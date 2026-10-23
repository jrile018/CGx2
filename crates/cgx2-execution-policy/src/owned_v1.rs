//! Version 1 source-lowering handoff for the two reviewed owned integer maps.
//!
//! Callers prove source purity/ownership before constructing `Input`. No managed
//! session or foreign handle can be sent through this API. Machine identity and
//! `execute_at`'s clock are trusted host inputs; production callers capture
//! `profile::current_machine()` locally and normally call `execute`.

use crate::profile::{self, MachineFingerprint, ProfileError};
use crate::{Backend, Capabilities, Decision, ExecuteError, MapError, Measurements, Policy};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

pub const REPORT_SCHEMA_VERSION: u32 = 1;
pub const MAP_SCHEMA_VERSION: u32 = 3;
pub const ZIP_SCHEMA_VERSION: u32 = 4;
pub const MAP_WORKLOAD_ID: &str = "checked-i64-owned-map-v1";
pub const ZIP_WORKLOAD_ID: &str = "checked-i64-owned-zip-map-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Map,
    ZipMap,
}

impl Operation {
    pub fn identity(self) -> (u32, &'static str) {
        match self {
            Self::Map => (MAP_SCHEMA_VERSION, MAP_WORKLOAD_ID),
            Self::ZipMap => (ZIP_SCHEMA_VERSION, ZIP_WORKLOAD_ID),
        }
    }
}

/// Consumes existing buffers without cloning them. All inputs are dropped before
/// return, including on errors. Results are published only as one complete Vec.
#[derive(Debug)]
pub enum Input {
    Map {
        values: Vec<i64>,
        multiplier: i64,
        addend: i64,
    },
    ZipMap {
        left: Vec<i64>,
        right: Vec<i64>,
        addend: i64,
    },
}

impl Input {
    pub fn operation(&self) -> Operation {
        match self {
            Self::Map { .. } => Operation::Map,
            Self::ZipMap { .. } => Operation::ZipMap,
        }
    }
    pub fn elements(&self) -> usize {
        match self {
            Self::Map { values, .. } => values.len(),
            Self::ZipMap { left, .. } => left.len(),
        }
    }
    fn capacity_bytes(&self) -> u128 {
        let capacities = match self {
            Self::Map { values, .. } => values.capacity() as u128,
            Self::ZipMap { left, right, .. } => left.capacity() as u128 + right.capacity() as u128,
        };
        capacities * size_of::<i64>() as u128
    }
    fn checked(&self) -> (crate::CheckedInputs<'_>, i64) {
        match self {
            Self::Map {
                values,
                multiplier,
                addend,
            } => (crate::CheckedInputs::Map(values, *multiplier), *addend),
            Self::ZipMap {
                left,
                right,
                addend,
            } => (crate::CheckedInputs::Zip(left, right), *addend),
        }
    }
}

/// Build exact-size evidence from timings of this API, not the borrowed API.
/// Samples use schema 3 (map) or 4 (zip) with the existing paired CSV layout.
/// Input creation/copy and profile loading precede timing; input drop, output
/// allocation, reporting, launch, worker buffers and joins are inside timing.
pub fn build_profile(
    operation: Operation,
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    elements: usize,
    now: SystemTime,
    ttl: Duration,
) -> Result<profile::Profile, ProfileError> {
    let profile = profile::build_profile_inner(
        csv,
        machine,
        workers,
        now,
        ttl,
        Some(elements),
        operation.identity(),
    )?;
    if profile.sequential_cost.estimate(elements).is_none()
        || profile.parallel_cost.estimate(elements).is_none()
    {
        return Err(ProfileError::InvalidProfile);
    }
    Ok(profile)
}

/// Opaque evidence: public legacy `Measurements` cannot be injected into v1.
#[derive(Clone, Debug)]
pub struct ValidatedProfile {
    saved: profile::Profile,
    measurements: Measurements,
    operation: Operation,
}

impl ValidatedProfile {
    pub fn load(
        operation: Operation,
        text: &str,
        csv: &str,
        machine: &MachineFingerprint,
        workers: usize,
        now: SystemTime,
    ) -> Result<Self, ProfileError> {
        let (schema, id) = operation.identity();
        let measurements =
            profile::load_profile_inner(text, csv, machine, workers, now, schema, id)?;
        let saved = profile::parse_profile(text)?;
        // No extrapolation/interpolation over gaps between measured lengths.
        if saved.minimum_elements != saved.maximum_elements {
            return Err(ProfileError::InvalidProfile);
        }
        if saved
            .sequential_cost
            .estimate(saved.minimum_elements)
            .is_none()
            || saved
                .parallel_cost
                .estimate(saved.minimum_elements)
                .is_none()
        {
            return Err(ProfileError::InvalidProfile);
        }
        Ok(Self {
            saved,
            measurements,
            operation,
        })
    }
    pub fn evidence(&self) -> &profile::Profile {
        &self.saved
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileRejection {
    Operation,
    Machine,
    Size,
    Workers,
    Age,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileStatus {
    Missing,
    Rejected(ProfileRejection),
    Accepted {
        raw_sample_hash: String,
        calibration_samples: usize,
        heldout_samples: usize,
    },
}

/// Captured once by the host; rechecks operation, size, machine, workers and age
/// on every invocation. Loading a profile is deliberately outside dispatch.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub parallel_workers: usize,
    pub machine: &'a MachineFingerprint,
    pub profile: Option<&'a ValidatedProfile>,
}

impl Context<'_> {
    fn select(
        &self,
        operation: Operation,
        elements: usize,
        now: SystemTime,
    ) -> (ProfileStatus, Option<Measurements>) {
        let Some(p) = self.profile else {
            return (ProfileStatus::Missing, None);
        };
        let epoch = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .map(|v| v.as_secs());
        let rejection = if p.operation != operation {
            Some(ProfileRejection::Operation)
        } else if p.saved.machine != *self.machine {
            Some(ProfileRejection::Machine)
        } else if elements != p.saved.minimum_elements {
            Some(ProfileRejection::Size)
        } else if self.parallel_workers != p.saved.worker_count {
            Some(ProfileRejection::Workers)
        } else if epoch
            .is_none_or(|t| t < p.saved.created_epoch_secs || t >= p.saved.expires_epoch_secs)
        {
            Some(ProfileRejection::Age)
        } else {
            None
        };
        if let Some(reason) = rejection {
            return (ProfileStatus::Rejected(reason), None);
        }
        let mut measurements = p.measurements.clone();
        // The existing planner has frozen borrowed-operation IDs. The private
        // adaptation is permitted only after owned identity validation above.
        measurements.workload_id = match operation {
            Operation::Map => profile::WORKLOAD_ID,
            Operation::ZipMap => profile::ZIP_WORKLOAD_ID,
        }
        .into();
        (
            ProfileStatus::Accepted {
                raw_sample_hash: p.saved.raw_sample_hash.clone(),
                calibration_samples: p.saved.calibration_samples,
                heldout_samples: p.saved.heldout_samples,
            },
            Some(measurements),
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Budgets {
    /// Observation only: a violation does not interrupt execution or undo work.
    pub elapsed_ns: Option<u128>,
    /// Preflight limit on requested i64 buffer bytes (defined by `Buffers`).
    pub requested_buffer_bytes: Option<u128>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetStatus {
    NotDeclared,
    NotEvaluated,
    Within,
    Exceeded,
}

fn assess(actual: u128, limit: Option<u128>) -> BudgetStatus {
    match limit {
        None => BudgetStatus::NotDeclared,
        Some(n) if actual <= n => BudgetStatus::Within,
        Some(_) => BudgetStatus::Exceeded,
    }
}

/// Conservative peak of retained input capacities + full requested output + all
/// requested private worker i64 chunks. No input copy occurs here. Parallel
/// assembly copies each successful chunk into output once. Allocator rounding,
/// Vec/handle metadata, thread stacks and report storage are excluded; this is
/// neither RSS nor a managed-region/no_gc budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Buffers {
    pub input_capacity_bytes: u128,
    pub output_requested_bytes: u128,
    pub worker_requested_bytes: u128,
    pub peak_requested_bytes: u128,
}

impl Buffers {
    fn for_input(input: &Input, backend: Backend) -> Self {
        let input_capacity_bytes = input.capacity_bytes();
        let output_requested_bytes = input.elements() as u128 * size_of::<i64>() as u128;
        let worker_requested_bytes = if backend == Backend::ParallelCpu {
            output_requested_bytes
        } else {
            0
        };
        Self {
            input_capacity_bytes,
            output_requested_bytes,
            worker_requested_bytes,
            peak_requested_bytes: input_capacity_bytes
                + output_requested_bytes
                + worker_requested_bytes,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Execution(ExecuteError),
    BufferBudgetExceeded { required: u128, limit: u128 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub schema_version: u32,
    pub machine: MachineFingerprint,
    pub operation: Operation,
    pub elements: usize,
    pub policy: Policy,
    pub profile: ProfileStatus,
    pub decision: Option<Decision>,
    /// Validated full-dispatch empirical intervals, even for explicit CPU
    /// policy. These are estimates, distinct from this call's observed latency.
    pub measured_costs: Option<MeasuredCosts>,
    /// Observed dispatch through joined workers and input cleanup. Profile
    /// loading, machine probing and caller input preparation are excluded.
    pub elapsed_ns: u128,
    /// Present after valid planning; an upper request bound, not allocations
    /// observed on an early error/cancel path.
    pub buffers: Option<Buffers>,
    pub budgets: Budgets,
    pub elapsed_budget: BudgetStatus,
    pub buffer_budget: BudgetStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasuredCosts {
    pub sequential: crate::CostInterval,
    pub parallel: crate::CostInterval,
}

/// Failure carries diagnostics/report, never partial output. All workers have
/// joined and all owned input buffers have been dropped before this is returned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub result: Result<Vec<i64>, Error>,
    pub report: Report,
}

pub fn execute(
    input: Input,
    policy: Policy,
    context: Context<'_>,
    cancellation: &AtomicBool,
    budgets: Budgets,
) -> Outcome {
    execute_at(
        input,
        policy,
        context,
        cancellation,
        budgets,
        SystemTime::now(),
    )
}

/// Deterministic clock injection for a trusted host/test; age is revalidated.
pub fn execute_at(
    input: Input,
    policy: Policy,
    context: Context<'_>,
    cancellation: &AtomicBool,
    budgets: Budgets,
    now: SystemTime,
) -> Outcome {
    let started = Instant::now();
    let machine = context.machine.clone();
    let operation = input.operation();
    let elements = input.elements();
    let (profile, measurements) = context.select(operation, elements, now);
    let measured_costs = measurements.as_ref().and_then(|m| {
        Some(MeasuredCosts {
            sequential: m.scalar.estimate(elements)?,
            parallel: m.parallel.estimate(elements)?,
        })
    });
    let mut decision = None;
    let mut buffers = None;
    let mut dispatched = false;
    let result = (|| {
        if let Input::ZipMap { left, right, .. } = &input {
            crate::check_lengths(left, right).map_err(ExecuteError::Map)?;
        }
        if policy == Policy::ParallelCpu {
            crate::validate_workers(elements, context.parallel_workers)
                .map_err(ExecuteError::Map)?;
        }
        let workload = match operation {
            Operation::Map => crate::Workload::checked_i64_map(elements),
            Operation::ZipMap => crate::Workload::checked_i64_zip_map(elements),
        };
        let caps = Capabilities {
            parallel_workers: context.parallel_workers,
            measurements,
            ..Capabilities::default()
        };
        let selected = crate::choose_at(policy, &workload, &caps, now).map_err(Error::from)?;
        decision = Some(selected);
        let requests = Buffers::for_input(&input, selected.selected);
        buffers = Some(requests);
        if cancellation.load(Ordering::Relaxed) {
            return Err(Error::from(ExecuteError::Map(MapError::Cancelled)));
        }
        if let Some(limit) = budgets.requested_buffer_bytes
            && requests.peak_requested_bytes > limit
        {
            return Err(Error::BufferBudgetExceeded {
                required: requests.peak_requested_bytes,
                limit,
            });
        }
        let (checked, addend) = input.checked();
        dispatched = true;
        match selected.selected {
            Backend::SequentialCpu => {
                crate::checked_sequential(checked, addend, cancellation, false)
            }
            Backend::ParallelCpu => crate::checked_parallel(
                checked,
                addend,
                selected.workers,
                cancellation,
                crate::ZipFaults::default(),
            ),
            Backend::Gpu => unreachable!("no GPU implementation"),
        }
        .map_err(|error| Error::Execution(ExecuteError::Map(error)))
    })();
    drop(input);
    // Final publication check also covers input cleanup. Validation/budget errors
    // retain their precedence; after dispatch cancellation wins worker outcomes.
    let result = if dispatched && cancellation.load(Ordering::Relaxed) {
        drop(result);
        Err(Error::Execution(ExecuteError::Map(MapError::Cancelled)))
    } else {
        result
    };
    let elapsed_ns = started.elapsed().as_nanos();
    let buffer_budget = buffers.map_or(
        if budgets.requested_buffer_bytes.is_some() {
            BudgetStatus::NotEvaluated
        } else {
            BudgetStatus::NotDeclared
        },
        |b| assess(b.peak_requested_bytes, budgets.requested_buffer_bytes),
    );
    Outcome {
        result,
        report: Report {
            schema_version: REPORT_SCHEMA_VERSION,
            machine,
            operation,
            elements,
            policy,
            profile,
            decision,
            measured_costs,
            elapsed_ns,
            buffers,
            budgets,
            elapsed_budget: assess(elapsed_ns, budgets.elapsed_ns),
            buffer_budget,
        },
    }
}

impl From<ExecuteError> for Error {
    fn from(error: ExecuteError) -> Self {
        Self::Execution(error)
    }
}
impl From<crate::PlanError> for Error {
    fn from(error: crate::PlanError) -> Self {
        Self::Execution(ExecuteError::Plan(error))
    }
}
