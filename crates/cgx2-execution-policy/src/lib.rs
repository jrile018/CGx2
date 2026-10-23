//! Source-independent execution policy for two reviewed, pure checked `i64` CPU maps.
//!
//! A decision is a safe choice under supplied capabilities and measurements, not a
//! claim that the chosen backend is globally optimal. No GPU backend exists here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

pub mod owned_v1;
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
    Cancelled,
    WorkerPanicked { chunk: usize },
    AllocationFailed,
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
    map_i64_checked_sequential_cancellable(input, multiplier, addend, &AtomicBool::new(false))
}

/// Checked scalar map with cooperative cancellation and fallible output allocation.
pub fn map_i64_checked_sequential_cancellable(
    input: &[i64],
    multiplier: i64,
    addend: i64,
    cancellation: &AtomicBool,
) -> Result<Vec<i64>, MapError> {
    checked_sequential(
        CheckedInputs::Map(input, multiplier),
        addend,
        cancellation,
        false,
    )
}

/// Legacy worker clamping and small-input sequential fallback are preserved.
pub fn map_i64_checked_parallel(
    input: &[i64],
    multiplier: i64,
    addend: i64,
    requested_workers: usize,
) -> Result<Vec<i64>, MapError> {
    let workers = requested_workers
        .min(MAX_WORKERS)
        .min(input.len() / MIN_CHUNK_ELEMENTS);
    let cancellation = AtomicBool::new(false);
    if workers < 2 || input.len() < MIN_PARALLEL_ELEMENTS {
        return map_i64_checked_sequential_cancellable(input, multiplier, addend, &cancellation);
    }
    map_i64_checked_parallel_cancellable(input, multiplier, addend, workers, &cancellation)
}

/// Strict worker validation, matching the cancellable zip primitive.
pub fn map_i64_checked_parallel_cancellable(
    input: &[i64],
    multiplier: i64,
    addend: i64,
    requested_workers: usize,
    cancellation: &AtomicBool,
) -> Result<Vec<i64>, MapError> {
    checked_parallel(
        CheckedInputs::Map(input, multiplier),
        addend,
        requested_workers,
        cancellation,
        ZipFaults::default(),
    )
}

/// Checked `left[i] * right[i] + addend` in input order.
pub fn zip_map_i64_checked_sequential(
    left: &[i64],
    right: &[i64],
    addend: i64,
) -> Result<Vec<i64>, MapError> {
    let cancellation = AtomicBool::new(false);
    zip_map_sequential_inner(left, right, addend, &cancellation, false)
}

fn zip_map_sequential_inner(
    left: &[i64],
    right: &[i64],
    addend: i64,
    cancellation: &AtomicBool,
    fail_allocation: bool,
) -> Result<Vec<i64>, MapError> {
    check_lengths(left, right)?;
    checked_sequential(
        CheckedInputs::Zip(left, right),
        addend,
        cancellation,
        fail_allocation,
    )
}

// Only the two reviewed integer primitives, never arbitrary closures or sessions.
#[derive(Clone, Copy)]
enum CheckedInputs<'a> {
    Map(&'a [i64], i64),
    Zip(&'a [i64], &'a [i64]),
}

impl CheckedInputs<'_> {
    fn len(self) -> usize {
        match self {
            Self::Map(input, _) | Self::Zip(input, _) => input.len(),
        }
    }
    fn value(self, index: usize, addend: i64) -> Result<i64, MapError> {
        match self {
            Self::Map(input, multiplier) => checked_value(input[index], multiplier, addend, index),
            Self::Zip(left, right) => checked_value(left[index], right[index], addend, index),
        }
    }
}

fn checked_sequential(
    input: CheckedInputs<'_>,
    addend: i64,
    cancellation: &AtomicBool,
    fail_allocation: bool,
) -> Result<Vec<i64>, MapError> {
    checked_sequential_with_faults(
        input,
        addend,
        cancellation,
        ZipFaults {
            fail_output_allocation: fail_allocation,
            ..ZipFaults::default()
        },
    )
}

fn checked_sequential_with_faults(
    input: CheckedInputs<'_>,
    addend: i64,
    cancellation: &AtomicBool,
    faults: ZipFaults<'_>,
) -> Result<Vec<i64>, MapError> {
    if cancellation.load(Ordering::Relaxed) {
        return Err(MapError::Cancelled);
    }
    let mut output = allocate_output(input.len(), faults.fail_output_allocation)?;
    let mut result = Ok(());
    for index in 0..input.len() {
        if faults.cancel_at_index == Some(index) {
            cancellation.store(true, Ordering::Relaxed);
        }
        if index.is_multiple_of(256) && cancellation.load(Ordering::Relaxed) {
            return Err(MapError::Cancelled);
        }
        match input.value(index, addend) {
            Ok(value) => output.push(value),
            Err(error) => {
                result = Err(error);
                break;
            }
        }
    }
    if cancellation.load(Ordering::Relaxed) {
        return Err(MapError::Cancelled);
    }
    result.map(|()| output)
}

fn check_lengths(left: &[i64], right: &[i64]) -> Result<(), MapError> {
    if left.len() == right.len() {
        Ok(())
    } else {
        Err(MapError::LengthMismatch {
            left: left.len(),
            right: right.len(),
        })
    }
}

fn allocate_output(len: usize, fail_allocation: bool) -> Result<Vec<i64>, MapError> {
    if fail_allocation {
        return Err(MapError::AllocationFailed);
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(len)
        .map_err(|_| MapError::AllocationFailed)?;
    Ok(output)
}

#[derive(Clone, Copy, Default)]
struct ZipFaults<'a> {
    panic_chunk: Option<usize>,
    cancel_chunk: Option<usize>,
    cancel_at_index: Option<usize>,
    fail_spawn_chunk: Option<usize>,
    fail_output_allocation: bool,
    fail_chunk_allocation: Option<usize>,
    completed_chunks: Option<&'a std::sync::atomic::AtomicUsize>,
}

/// Scoped pure workers evaluate chunks in input order. Length and worker
/// validation precede cancellation. Once launched, every worker is joined;
/// the first failed chunk wins, and cancellation observed before return wins
/// over worker outcomes. An error drops all partial output.
pub fn zip_map_i64_checked_parallel(
    left: &[i64],
    right: &[i64],
    addend: i64,
    requested_workers: usize,
) -> Result<Vec<i64>, MapError> {
    let cancellation = AtomicBool::new(false);
    zip_map_i64_checked_parallel_cancellable(left, right, addend, requested_workers, &cancellation)
}

/// A cancellation request may be set from another thread. The call returns
/// `Cancelled` if the flag is set before its final check, after joining workers.
pub fn zip_map_i64_checked_parallel_cancellable(
    left: &[i64],
    right: &[i64],
    addend: i64,
    requested_workers: usize,
    cancellation: &AtomicBool,
) -> Result<Vec<i64>, MapError> {
    zip_map_parallel_inner(
        left,
        right,
        addend,
        requested_workers,
        cancellation,
        ZipFaults::default(),
    )
}

fn zip_map_parallel_inner(
    left: &[i64],
    right: &[i64],
    addend: i64,
    requested_workers: usize,
    cancellation: &AtomicBool,
    faults: ZipFaults<'_>,
) -> Result<Vec<i64>, MapError> {
    check_lengths(left, right)?;
    checked_parallel(
        CheckedInputs::Zip(left, right),
        addend,
        requested_workers,
        cancellation,
        faults,
    )
}

fn validate_workers(elements: usize, requested: usize) -> Result<(), MapError> {
    if !(2..=MAX_WORKERS).contains(&requested)
        || elements < MIN_PARALLEL_ELEMENTS
        || requested > elements / MIN_CHUNK_ELEMENTS
    {
        return Err(MapError::InvalidWorkers { requested });
    }
    Ok(())
}

fn checked_parallel(
    input: CheckedInputs<'_>,
    addend: i64,
    requested_workers: usize,
    cancellation: &AtomicBool,
    faults: ZipFaults<'_>,
) -> Result<Vec<i64>, MapError> {
    validate_workers(input.len(), requested_workers)?;
    if cancellation.load(Ordering::Relaxed) {
        return Err(MapError::Cancelled);
    }
    let mut output = allocate_output(input.len(), faults.fail_output_allocation)?;
    let chunk_size = input.len().div_ceil(requested_workers);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        handles
            .try_reserve_exact(requested_workers)
            .map_err(|_| MapError::AllocationFailed)?;
        let mut first_error = None;
        for chunk in 0..input.len().div_ceil(chunk_size) {
            if faults.fail_spawn_chunk == Some(chunk) {
                first_error = Some(MapError::AllocationFailed);
                break;
            }
            let start = chunk * chunk_size;
            let end = start.saturating_add(chunk_size).min(input.len());
            // Unlike Scope::spawn, resource refusal is returned, not panicked.
            let handle = std::thread::Builder::new().spawn_scoped(scope, move || {
                if faults.panic_chunk == Some(chunk) {
                    panic!("injected checked worker panic");
                }
                if faults.cancel_chunk == Some(chunk) {
                    cancellation.store(true, Ordering::Relaxed);
                }
                let result = (|| {
                    let mut values =
                        allocate_output(end - start, faults.fail_chunk_allocation == Some(chunk))?;
                    for index in start..end {
                        if faults.cancel_at_index == Some(index) {
                            cancellation.store(true, Ordering::Relaxed);
                        }
                        if (index - start).is_multiple_of(256)
                            && cancellation.load(Ordering::Relaxed)
                        {
                            return Err(MapError::Cancelled);
                        }
                        values.push(input.value(index, addend)?);
                    }
                    Ok(values)
                })();
                if let Some(completed) = faults.completed_chunks {
                    completed.fetch_add(1, Ordering::Relaxed);
                }
                result
            });
            match handle {
                Ok(handle) => handles.push(handle),
                Err(_) => {
                    first_error = Some(MapError::AllocationFailed);
                    break;
                }
            }
        }
        let mut worker_error = None;
        for (chunk, handle) in handles.into_iter().enumerate() {
            match handle.join() {
                Ok(Ok(values)) => output.extend(values),
                Ok(Err(error)) if worker_error.is_none() => worker_error = Some(error),
                Err(_) if worker_error.is_none() => {
                    worker_error = Some(MapError::WorkerPanicked { chunk })
                }
                _ => {}
            }
        }
        if cancellation.load(Ordering::Relaxed) {
            Err(MapError::Cancelled)
        } else {
            worker_error.or(first_error).map_or(Ok(output), Err)
        }
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

/// Scalar counterpart of [`execute_zip_map_cancellable`]. Validation precedes
/// cancellation; unlike the legacy direct map, explicit workers must be feasible.
pub fn execute_map_cancellable(
    input: &[i64],
    multiplier: i64,
    addend: i64,
    policy: Policy,
    caps: &Capabilities,
    cancellation: &AtomicBool,
) -> Result<Execution, ExecuteError> {
    if policy == Policy::ParallelCpu {
        validate_workers(input.len(), caps.parallel_workers).map_err(ExecuteError::Map)?;
    }
    let decision = choose(policy, &Workload::checked_i64_map(input.len()), caps)
        .map_err(ExecuteError::Plan)?;
    let values = match decision.selected {
        Backend::SequentialCpu => {
            map_i64_checked_sequential_cancellable(input, multiplier, addend, cancellation)
        }
        Backend::ParallelCpu => map_i64_checked_parallel_cancellable(
            input,
            multiplier,
            addend,
            decision.workers,
            cancellation,
        ),
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
    let cancellation = AtomicBool::new(false);
    execute_zip_map_cancellable(left, right, addend, policy, caps, &cancellation)
}

/// Plan and execute the reviewed primitive with a cancellation flag. A future
/// checked `.cg` lowering can pass its runtime cancellation flag here after
/// proving pure, independent `i64` work and excluding managed session state.
pub fn execute_zip_map_cancellable(
    left: &[i64],
    right: &[i64],
    addend: i64,
    policy: Policy,
    caps: &Capabilities,
    cancellation: &AtomicBool,
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
        Backend::SequentialCpu => {
            zip_map_sequential_inner(left, right, addend, cancellation, false)
        }
        Backend::ParallelCpu => zip_map_i64_checked_parallel_cancellable(
            left,
            right,
            addend,
            decision.workers,
            cancellation,
        ),
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

    #[test]
    fn injected_failures_never_return_partial_output() {
        let left = vec![2; 4_099];
        let right = vec![3; left.len()];
        let cancellation = AtomicBool::new(false);
        assert_eq!(
            zip_map_sequential_inner(&left, &right, 7, &cancellation, true),
            Err(MapError::AllocationFailed)
        );
        assert_eq!(
            zip_map_parallel_inner(
                &left,
                &right,
                7,
                3,
                &cancellation,
                ZipFaults {
                    fail_output_allocation: true,
                    ..ZipFaults::default()
                }
            ),
            Err(MapError::AllocationFailed)
        );
        assert_eq!(
            zip_map_parallel_inner(
                &left,
                &right,
                7,
                3,
                &cancellation,
                ZipFaults {
                    panic_chunk: Some(1),
                    ..ZipFaults::default()
                }
            ),
            Err(MapError::WorkerPanicked { chunk: 1 })
        );
        assert_eq!(
            zip_map_parallel_inner(
                &left,
                &right,
                7,
                3,
                &cancellation,
                ZipFaults {
                    fail_chunk_allocation: Some(1),
                    ..ZipFaults::default()
                }
            ),
            Err(MapError::AllocationFailed)
        );
    }

    #[test]
    fn all_workers_join_and_first_failed_chunk_wins() {
        let mut left = vec![2; 4_099];
        let right = vec![3; left.len()];
        let cancellation = AtomicBool::new(false);
        let completed = std::sync::atomic::AtomicUsize::new(0);
        let chunk_size = left.len().div_ceil(3);
        left[chunk_size + 1] = i64::MAX;
        left[2 * chunk_size] = i64::MAX;
        assert_eq!(
            zip_map_parallel_inner(&left, &right, 0, 3, &cancellation, ZipFaults::default()),
            Err(MapError::Overflow {
                index: chunk_size + 1
            })
        );
        assert_eq!(
            zip_map_parallel_inner(
                &left,
                &right,
                0,
                3,
                &cancellation,
                ZipFaults {
                    panic_chunk: Some(0),
                    completed_chunks: Some(&completed),
                    ..ZipFaults::default()
                }
            ),
            Err(MapError::WorkerPanicked { chunk: 0 })
        );
        assert_eq!(completed.load(Ordering::Relaxed), 2);
        assert_eq!(
            zip_map_parallel_inner(
                &left,
                &right,
                0,
                3,
                &cancellation,
                ZipFaults {
                    panic_chunk: Some(2),
                    ..ZipFaults::default()
                }
            ),
            Err(MapError::Overflow {
                index: chunk_size + 1
            })
        );
    }

    #[test]
    fn cancellation_after_launch_wins_over_worker_failures() {
        let mut left = vec![2; 4_099];
        let right = vec![3; left.len()];
        left[1] = i64::MAX;
        let cancellation = AtomicBool::new(false);
        assert_eq!(
            zip_map_parallel_inner(
                &left,
                &right,
                0,
                3,
                &cancellation,
                ZipFaults {
                    cancel_chunk: Some(2),
                    ..ZipFaults::default()
                }
            ),
            Err(MapError::Cancelled)
        );
        assert!(cancellation.load(Ordering::Relaxed));
    }
    #[test]
    fn both_primitives_cancel_after_computation_and_join_on_spawn_refusal() {
        let left = vec![2; 4099];
        let right = vec![3; left.len()];
        for input in [
            CheckedInputs::Map(&left, 3),
            CheckedInputs::Zip(&left, &right),
        ] {
            for index in [0, 1, 255, 256, 4098] {
                let cancellation = AtomicBool::new(false);
                assert_eq!(
                    checked_sequential_with_faults(
                        input,
                        7,
                        &cancellation,
                        ZipFaults {
                            cancel_at_index: Some(index),
                            ..ZipFaults::default()
                        }
                    ),
                    Err(MapError::Cancelled)
                );
                let completed = std::sync::atomic::AtomicUsize::new(0);
                cancellation.store(false, Ordering::Relaxed);
                assert_eq!(
                    checked_parallel(
                        input,
                        7,
                        3,
                        &cancellation,
                        ZipFaults {
                            cancel_at_index: Some(index),
                            completed_chunks: Some(&completed),
                            ..ZipFaults::default()
                        }
                    ),
                    Err(MapError::Cancelled)
                );
                assert_eq!(completed.load(Ordering::Relaxed), 3);
            }
            for fail_chunk in [0, 1, 2] {
                let completed = std::sync::atomic::AtomicUsize::new(0);
                assert_eq!(
                    checked_parallel(
                        input,
                        7,
                        3,
                        &AtomicBool::new(false),
                        ZipFaults {
                            fail_spawn_chunk: Some(fail_chunk),
                            completed_chunks: Some(&completed),
                            ..ZipFaults::default()
                        }
                    ),
                    Err(MapError::AllocationFailed)
                );
                assert_eq!(completed.load(Ordering::Relaxed), fail_chunk);
            }
            for faults in [
                ZipFaults {
                    panic_chunk: Some(1),
                    ..ZipFaults::default()
                },
                ZipFaults {
                    fail_chunk_allocation: Some(1),
                    ..ZipFaults::default()
                },
                ZipFaults {
                    fail_output_allocation: true,
                    ..ZipFaults::default()
                },
            ] {
                let completed = std::sync::atomic::AtomicUsize::new(0);
                let result = checked_parallel(
                    input,
                    7,
                    3,
                    &AtomicBool::new(false),
                    ZipFaults {
                        completed_chunks: Some(&completed),
                        ..faults
                    },
                );
                let expected = if faults.panic_chunk.is_some() {
                    MapError::WorkerPanicked { chunk: 1 }
                } else {
                    MapError::AllocationFailed
                };
                assert_eq!(result, Err(expected));
                assert_eq!(
                    completed.load(Ordering::Relaxed),
                    if faults.fail_output_allocation {
                        0
                    } else if faults.panic_chunk.is_some() {
                        2
                    } else {
                        3
                    }
                );
            }
        }
    }
}
