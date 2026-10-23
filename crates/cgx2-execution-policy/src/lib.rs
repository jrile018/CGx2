//! Source-independent execution policy for two reviewed, pure checked `i64` CPU maps.
//!
//! A decision is a safe choice under supplied capabilities and measurements, not a
//! claim that the chosen backend is globally optimal. No GPU backend exists here.

use std::time::SystemTime;

pub mod profile;

/// No scoped workers are launched below this input size.
pub const MIN_PARALLEL_ELEMENTS: usize = 2_048;
/// Each scoped worker receives at least this many elements.
pub const MIN_CHUNK_ELEMENTS: usize = 1_024;
/// Hard bound on scoped worker count, even if the caller requests more.
pub const MAX_WORKERS: usize = 32;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Policy {
    #[default]
    Auto,
    SequentialCpu,
    ParallelCpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    SequentialCpu,
    ParallelCpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasonCode {
    ExplicitSequential,
    ExplicitParallel,
    ParallelUnavailable,
    GpuUnavailable,
    SafetyExclusion,
    SmallWorkload,
    MissingMeasurements,
    StaleMeasurements,
    NoMeasuredBenefit,
    MeasuredParallelBenefit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanError {
    UnsupportedBackend {
        requested: Backend,
        reason: ReasonCode,
    },
    UnsafeWorkload {
        requested: Backend,
        reason: ReasonCode,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    CheckedI64Elementwise,
    CheckedI64ZipMap,
    Reduction,
    Other,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MemoryActivity {
    #[default]
    None,
    Owned,
    ManagedSession,
    Foreign,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Effects {
    pub observable_io: bool,
    pub foreign_call: bool,
    pub allocation: bool,
    pub collection: bool,
}

impl Effects {
    fn is_pure(self) -> bool {
        !self.observable_io && !self.foreign_call && !self.allocation && !self.collection
    }
}

/// Typed assertions supplied by a future checked IR or a reviewed library call.
/// The current planner trusts these fields only for the two checked CPU maps.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workload {
    pub elements: usize,
    /// Input plus output bytes, excluding allocator overhead.
    pub bytes: u128,
    pub operation: OperationKind,
    pub independent: bool,
    /// Reserved for a future reduction contract; it does not enable reductions here.
    pub associative: bool,
    pub order_sensitive: bool,
    pub shared_mutable_alias: bool,
    pub memory_activity: MemoryActivity,
    pub effects: Effects,
    pub no_gc: bool,
}

impl Workload {
    pub fn checked_i64_map(elements: usize) -> Self {
        Self {
            elements,
            bytes: elements as u128 * 2 * size_of::<i64>() as u128,
            operation: OperationKind::CheckedI64Elementwise,
            independent: true,
            associative: false,
            order_sensitive: false,
            shared_mutable_alias: false,
            memory_activity: MemoryActivity::None,
            effects: Effects::default(),
            no_gc: false,
        }
    }

    pub fn checked_i64_zip_map(elements: usize) -> Self {
        Self {
            operation: OperationKind::CheckedI64ZipMap,
            bytes: elements as u128 * 3 * size_of::<i64>() as u128,
            ..Self::checked_i64_map(elements)
        }
    }

    fn safe_for_parallel(&self) -> bool {
        matches!(
            self.operation,
            OperationKind::CheckedI64Elementwise | OperationKind::CheckedI64ZipMap
        ) && self.bytes
            == self.elements as u128
                * if self.operation == OperationKind::CheckedI64ZipMap {
                    3
                } else {
                    2
                }
                * size_of::<i64>() as u128
            && self.independent
            && !self.order_sensitive
            && !self.shared_mutable_alias
            && self.memory_activity == MemoryActivity::None
            && self.effects.is_pure()
            && !self.no_gc
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CostInterval {
    pub lower_ns: u128,
    pub upper_ns: u128,
}

impl CostInterval {
    fn valid(self) -> bool {
        self.lower_ns <= self.upper_ns
    }

    fn add(self, other: Self) -> Self {
        Self {
            lower_ns: self.lower_ns.saturating_add(other.lower_ns),
            upper_ns: self.upper_ns.saturating_add(other.upper_ns),
        }
    }

    fn times(self, n: usize) -> Self {
        Self {
            lower_ns: self.lower_ns.saturating_mul(n as u128),
            upper_ns: self.upper_ns.saturating_mul(n as u128),
        }
    }
}

/// End-to-end cost components in nanoseconds. Transfer, synchronization, and
/// memory pressure are explicit even when measured as zero for this CPU map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CostProfile {
    pub launch_setup: CostInterval,
    pub cpu_work_per_element: CostInterval,
    pub host_device_transfer: CostInterval,
    pub synchronization: CostInterval,
    pub memory_pressure: CostInterval,
}

impl CostProfile {
    pub fn estimate(self, elements: usize) -> Option<CostInterval> {
        let fields = [
            self.launch_setup,
            self.cpu_work_per_element,
            self.host_device_transfer,
            self.synchronization,
            self.memory_pressure,
        ];
        if fields.iter().any(|part| !part.valid()) {
            return None;
        }
        Some(
            self.launch_setup
                .add(self.cpu_work_per_element.times(elements))
                .add(self.host_device_transfer)
                .add(self.synchronization)
                .add(self.memory_pressure),
        )
    }
}

/// Measurements must cover the input size and the selected worker count.
/// At least three samples and a future `valid_until` are required for `Auto`.
#[derive(Clone, Debug)]
pub struct Measurements {
    /// Exact reviewed primitive whose end-to-end timings were sampled.
    pub workload_id: String,
    pub valid_until: SystemTime,
    pub minimum_elements: usize,
    pub maximum_elements: usize,
    pub sample_count: usize,
    pub worker_count: usize,
    pub scalar: CostProfile,
    pub parallel: CostProfile,
}

/// GPU capability is intentionally unconstructible outside this crate until a
/// real executable backend and probe exist. A cost estimate alone cannot enable it.
#[derive(Clone, Copy, Debug, Default)]
pub struct GpuCapability {
    estimated_cost: Option<CostProfile>,
}

impl GpuCapability {
    /// Advisory estimate only; there is no executable GPU backend.
    pub fn estimated_cost(self, elements: usize) -> Option<CostInterval> {
        self.estimated_cost?.estimate(elements)
    }
}

#[derive(Clone, Debug)]
pub struct Capabilities {
    /// Set to 1 by default. Values above 1 enable explicit CPU parallel requests.
    pub parallel_workers: usize,
    pub measurements: Option<Measurements>,
    pub gpu: GpuCapability,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            parallel_workers: 1,
            measurements: None,
            gpu: GpuCapability::default(),
        }
    }
}

impl Capabilities {
    fn supported_workers(&self, elements: usize) -> usize {
        self.parallel_workers
            .min(MAX_WORKERS)
            .min(elements / MIN_CHUNK_ELEMENTS)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CostStatus {
    Uncalibrated,
    Calibrated {
        sequential: CostInterval,
        selected: CostInterval,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Decision {
    pub selected: Backend,
    pub reason: ReasonCode,
    pub cost: CostStatus,
    /// Scoped workers requested by the decision, or 1 for sequential CPU.
    pub workers: usize,
}

fn sequential(reason: ReasonCode, cost: CostStatus) -> Decision {
    Decision {
        selected: Backend::SequentialCpu,
        reason,
        cost,
        workers: 1,
    }
}

fn measured_costs(
    workload: &Workload,
    caps: &Capabilities,
    workers: usize,
    now: SystemTime,
) -> Result<(CostInterval, CostInterval), ReasonCode> {
    let Some(m) = &caps.measurements else {
        return Err(ReasonCode::MissingMeasurements);
    };
    let expected_id = match workload.operation {
        OperationKind::CheckedI64Elementwise => profile::WORKLOAD_ID,
        OperationKind::CheckedI64ZipMap => profile::ZIP_WORKLOAD_ID,
        _ => return Err(ReasonCode::MissingMeasurements),
    };
    if m.workload_id != expected_id {
        return Err(ReasonCode::MissingMeasurements);
    }
    if now >= m.valid_until {
        return Err(ReasonCode::StaleMeasurements);
    }
    if m.sample_count < 3
        || m.minimum_elements > m.maximum_elements
        || workload.elements < m.minimum_elements
        || workload.elements > m.maximum_elements
        || m.worker_count != workers
    {
        return Err(ReasonCode::MissingMeasurements);
    }
    let Some(scalar) = m.scalar.estimate(workload.elements) else {
        return Err(ReasonCode::MissingMeasurements);
    };
    let Some(parallel) = m.parallel.estimate(workload.elements) else {
        return Err(ReasonCode::MissingMeasurements);
    };
    Ok((scalar, parallel))
}

/// Select a backend for the one reviewed map contract. `Auto` requires
/// conservative interval dominance: parallel upper cost < sequential lower cost.
pub fn choose(
    policy: Policy,
    workload: &Workload,
    caps: &Capabilities,
) -> Result<Decision, PlanError> {
    choose_at(policy, workload, caps, SystemTime::now())
}

/// Deterministic planner entry point for callers that already have a trusted clock.
/// Ordinary execution uses [`choose`] and the system clock.
pub fn choose_at(
    policy: Policy,
    workload: &Workload,
    caps: &Capabilities,
    now: SystemTime,
) -> Result<Decision, PlanError> {
    if policy == Policy::SequentialCpu {
        return Ok(sequential(
            ReasonCode::ExplicitSequential,
            CostStatus::Uncalibrated,
        ));
    }
    if policy == Policy::Gpu {
        return Err(PlanError::UnsupportedBackend {
            requested: Backend::Gpu,
            reason: ReasonCode::GpuUnavailable,
        });
    }
    if !workload.safe_for_parallel() {
        return match policy {
            Policy::Auto => Ok(sequential(
                ReasonCode::SafetyExclusion,
                CostStatus::Uncalibrated,
            )),
            _ => Err(PlanError::UnsafeWorkload {
                requested: Backend::ParallelCpu,
                reason: ReasonCode::SafetyExclusion,
            }),
        };
    }
    if caps.parallel_workers < 2 {
        return match policy {
            Policy::Auto => Ok(sequential(
                ReasonCode::ParallelUnavailable,
                CostStatus::Uncalibrated,
            )),
            _ => Err(PlanError::UnsupportedBackend {
                requested: Backend::ParallelCpu,
                reason: ReasonCode::ParallelUnavailable,
            }),
        };
    }
    if workload.elements < MIN_PARALLEL_ELEMENTS {
        return match policy {
            Policy::Auto => Ok(sequential(
                ReasonCode::SmallWorkload,
                CostStatus::Uncalibrated,
            )),
            _ => Err(PlanError::UnsupportedBackend {
                requested: Backend::ParallelCpu,
                reason: ReasonCode::SmallWorkload,
            }),
        };
    }
    let workers = caps.supported_workers(workload.elements);
    if workers < 2 {
        return Ok(sequential(
            ReasonCode::SmallWorkload,
            CostStatus::Uncalibrated,
        ));
    }
    let costs = measured_costs(workload, caps, workers, now);
    if policy == Policy::ParallelCpu {
        return Ok(Decision {
            selected: Backend::ParallelCpu,
            reason: ReasonCode::ExplicitParallel,
            cost: costs.map_or(CostStatus::Uncalibrated, |(s, p)| CostStatus::Calibrated {
                sequential: s,
                selected: p,
            }),
            workers,
        });
    }
    match costs {
        Err(reason) => Ok(sequential(reason, CostStatus::Uncalibrated)),
        Ok((scalar, parallel)) if parallel.upper_ns < scalar.lower_ns => Ok(Decision {
            selected: Backend::ParallelCpu,
            reason: ReasonCode::MeasuredParallelBenefit,
            cost: CostStatus::Calibrated {
                sequential: scalar,
                selected: parallel,
            },
            workers,
        }),
        Ok((scalar, _)) => Ok(sequential(
            ReasonCode::NoMeasuredBenefit,
            CostStatus::Calibrated {
                sequential: scalar,
                selected: scalar,
            },
        )),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapError {
    Overflow { index: usize },
    LengthMismatch { left: usize, right: usize },
    InvalidWorkers { requested: usize },
}

fn checked_value(value: i64, multiplier: i64, addend: i64, index: usize) -> Result<i64, MapError> {
    value
        .checked_mul(multiplier)
        .and_then(|v| v.checked_add(addend))
        .ok_or(MapError::Overflow { index })
}

/// Apply `value * multiplier + addend` with checked operations in source order.
pub fn map_i64_checked_sequential(
    input: &[i64],
    multiplier: i64,
    addend: i64,
) -> Result<Vec<i64>, MapError> {
    input
        .iter()
        .enumerate()
        .map(|(index, &value)| checked_value(value, multiplier, addend, index))
        .collect()
}

/// Scoped workers evaluate disjoint chunks. Results are joined in source order,
/// preserving the first overflow index even when later chunks finish first.
pub fn map_i64_checked_parallel(
    input: &[i64],
    multiplier: i64,
    addend: i64,
    requested_workers: usize,
) -> Result<Vec<i64>, MapError> {
    let workers = requested_workers
        .min(MAX_WORKERS)
        .min(input.len() / MIN_CHUNK_ELEMENTS);
    if workers < 2 || input.len() < MIN_PARALLEL_ELEMENTS {
        return map_i64_checked_sequential(input, multiplier, addend);
    }
    let chunk_size = input.len().div_ceil(workers);
    std::thread::scope(|scope| {
        let handles: Vec<_> = input
            .chunks(chunk_size)
            .enumerate()
            .map(|(chunk, slice)| {
                scope.spawn(move || {
                    slice
                        .iter()
                        .enumerate()
                        .map(|(offset, &value)| {
                            checked_value(value, multiplier, addend, chunk * chunk_size + offset)
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .collect();
        let mut output = Vec::with_capacity(input.len());
        let mut first_error = None;
        for handle in handles {
            match handle.join().expect("pure map worker panicked") {
                Ok(chunk) => output.extend(chunk),
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        first_error.map_or(Ok(output), Err)
    })
}

/// Checked `left[i] * right[i] + addend` in input order.
pub fn zip_map_i64_checked_sequential(
    left: &[i64],
    right: &[i64],
    addend: i64,
) -> Result<Vec<i64>, MapError> {
    if left.len() != right.len() {
        return Err(MapError::LengthMismatch {
            left: left.len(),
            right: right.len(),
        });
    }
    left.iter()
        .zip(right)
        .enumerate()
        .map(|(index, (&l, &r))| checked_value(l, r, addend, index))
        .collect()
}

/// Scoped pure workers; chunk joins preserve the lowest overflow index.
/// An infeasible request is rejected before a worker is launched.
pub fn zip_map_i64_checked_parallel(
    left: &[i64],
    right: &[i64],
    addend: i64,
    requested_workers: usize,
) -> Result<Vec<i64>, MapError> {
    if left.len() != right.len() {
        return Err(MapError::LengthMismatch {
            left: left.len(),
            right: right.len(),
        });
    }
    if !(2..=MAX_WORKERS).contains(&requested_workers)
        || left.len() < MIN_PARALLEL_ELEMENTS
        || requested_workers > left.len() / MIN_CHUNK_ELEMENTS
    {
        return Err(MapError::InvalidWorkers {
            requested: requested_workers,
        });
    }
    let chunk_size = left.len().div_ceil(requested_workers);
    std::thread::scope(|scope| {
        let handles: Vec<_> = left
            .chunks(chunk_size)
            .zip(right.chunks(chunk_size))
            .enumerate()
            .map(|(chunk, (l, r))| {
                scope.spawn(move || {
                    l.iter()
                        .zip(r)
                        .enumerate()
                        .map(|(offset, (&a, &b))| {
                            checked_value(a, b, addend, chunk * chunk_size + offset)
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .collect();
        let mut output = Vec::with_capacity(left.len());
        let mut first_error = None;
        for handle in handles {
            match handle.join().expect("pure zip map worker panicked") {
                Ok(chunk) => output.extend(chunk),
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        first_error.map_or(Ok(output), Err)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecuteError {
    Plan(PlanError),
    Map(MapError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Execution {
    pub decision: Decision,
    pub values: Vec<i64>,
}

/// Plan and run the reviewed primitive. The workload is built from the actual
/// slice so callers cannot understate its size or safety conditions.
pub fn execute_map(
    input: &[i64],
    multiplier: i64,
    addend: i64,
    policy: Policy,
    caps: &Capabilities,
) -> Result<Execution, ExecuteError> {
    let workload = Workload::checked_i64_map(input.len());
    let decision = choose(policy, &workload, caps).map_err(ExecuteError::Plan)?;
    let values = match decision.selected {
        Backend::SequentialCpu => map_i64_checked_sequential(input, multiplier, addend),
        Backend::ParallelCpu => {
            map_i64_checked_parallel(input, multiplier, addend, decision.workers)
        }
        Backend::Gpu => unreachable!("GPU has no executable backend"),
    }
    .map_err(ExecuteError::Map)?;
    Ok(Execution { decision, values })
}

/// Plan and execute the standalone two-input checked CPU primitive.
pub fn execute_zip_map(
    left: &[i64],
    right: &[i64],
    addend: i64,
    policy: Policy,
    caps: &Capabilities,
) -> Result<Execution, ExecuteError> {
    if left.len() != right.len() {
        return Err(ExecuteError::Map(MapError::LengthMismatch {
            left: left.len(),
            right: right.len(),
        }));
    }
    if policy == Policy::ParallelCpu
        && (!(2..=MAX_WORKERS).contains(&caps.parallel_workers)
            || left.len() < MIN_PARALLEL_ELEMENTS
            || caps.parallel_workers > left.len() / MIN_CHUNK_ELEMENTS)
    {
        return Err(ExecuteError::Map(MapError::InvalidWorkers {
            requested: caps.parallel_workers,
        }));
    }
    let workload = Workload::checked_i64_zip_map(left.len());
    let decision = choose(policy, &workload, caps).map_err(ExecuteError::Plan)?;
    let values = match decision.selected {
        Backend::SequentialCpu => zip_map_i64_checked_sequential(left, right, addend),
        Backend::ParallelCpu => zip_map_i64_checked_parallel(left, right, addend, decision.workers),
        Backend::Gpu => unreachable!("GPU backend cannot be selected"),
    }
    .map_err(ExecuteError::Map)?;
    Ok(Execution { decision, values })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_gpu_cost_never_enables_execution() {
        let zero = CostInterval {
            lower_ns: 0,
            upper_ns: 0,
        };
        let mock = GpuCapability {
            estimated_cost: Some(CostProfile {
                launch_setup: zero,
                cpu_work_per_element: zero,
                host_device_transfer: zero,
                synchronization: zero,
                memory_pressure: zero,
            }),
        };
        let caps = Capabilities {
            gpu: mock,
            ..Capabilities::default()
        };
        let workload = Workload::checked_i64_map(100_000);
        assert_eq!(mock.estimated_cost(workload.elements), Some(zero));
        assert_eq!(
            choose(Policy::Auto, &workload, &caps).unwrap().selected,
            Backend::SequentialCpu
        );
        assert_eq!(
            choose(Policy::Gpu, &workload, &caps).unwrap_err(),
            PlanError::UnsupportedBackend {
                requested: Backend::Gpu,
                reason: ReasonCode::GpuUnavailable
            }
        );
    }
}
