use cgx2_execution_policy::{
    Backend, Capabilities, CostInterval, CostProfile, CostStatus, Effects, Measurements,
    MemoryActivity, OperationKind, PlanError, Policy, ReasonCode, Workload, choose, execute_map,
};
use std::time::{Duration, SystemTime};

fn interval(lower_ns: u128, upper_ns: u128) -> CostInterval {
    CostInterval { lower_ns, upper_ns }
}

fn profile(
    setup: u128,
    per_element: u128,
    transfer: u128,
    sync: u128,
    memory: u128,
) -> CostProfile {
    CostProfile {
        launch_setup: interval(setup, setup),
        cpu_work_per_element: interval(per_element, per_element),
        host_device_transfer: interval(transfer, transfer),
        synchronization: interval(sync, sync),
        memory_pressure: interval(memory, memory),
    }
}

fn measured() -> Capabilities {
    Capabilities {
        parallel_workers: 4,
        measurements: Some(Measurements {
            valid_until: SystemTime::now() + Duration::from_secs(3600),
            minimum_elements: 1,
            maximum_elements: 100_000,
            sample_count: 5,
            worker_count: 4,
            scalar: profile(10, 10, 0, 0, 0),
            parallel: profile(35_000, 2, 500, 500, 100),
        }),
        ..Capabilities::default()
    }
}

#[test]
fn defaults_and_conservative_selection() {
    assert_eq!(Policy::default(), Policy::Auto);
    let w = Workload::checked_i64_map(20_000);
    let d = choose(Policy::Auto, &w, &Capabilities::default()).unwrap();
    assert_eq!(
        (d.selected, d.reason, d.cost),
        (
            Backend::SequentialCpu,
            ReasonCode::ParallelUnavailable,
            CostStatus::Uncalibrated
        )
    );
    let cores_only = Capabilities {
        parallel_workers: 8,
        ..Capabilities::default()
    };
    assert_eq!(
        choose(Policy::Auto, &w, &cores_only).unwrap().reason,
        ReasonCode::MissingMeasurements
    );
    assert_eq!(
        choose(Policy::Gpu, &w, &measured()).unwrap_err(),
        PlanError::UnsupportedBackend {
            requested: Backend::Gpu,
            reason: ReasonCode::GpuUnavailable
        }
    );
    assert_eq!(
        choose(Policy::ParallelCpu, &w, &Capabilities::default()).unwrap_err(),
        PlanError::UnsupportedBackend {
            requested: Backend::ParallelCpu,
            reason: ReasonCode::ParallelUnavailable
        }
    );
}

#[test]
fn measured_costs_and_threshold() {
    let caps = measured();
    let small = choose(Policy::Auto, &Workload::checked_i64_map(4_096), &caps).unwrap();
    assert_eq!(small.reason, ReasonCode::NoMeasuredBenefit);
    let large = choose(Policy::Auto, &Workload::checked_i64_map(20_000), &caps).unwrap();
    assert_eq!(
        (large.selected, large.reason),
        (Backend::ParallelCpu, ReasonCode::MeasuredParallelBenefit)
    );
    let CostStatus::Calibrated {
        sequential,
        selected,
    } = large.cost
    else {
        panic!("expected calibrated costs")
    };
    assert_eq!(sequential, interval(200_010, 200_010));
    assert_eq!(selected, interval(76_100, 76_100));
}

#[test]
fn stale_or_out_of_range_measurement_falls_back() {
    let mut caps = measured();
    caps.measurements.as_mut().unwrap().valid_until = SystemTime::UNIX_EPOCH;
    assert_eq!(
        choose(Policy::Auto, &Workload::checked_i64_map(20_000), &caps)
            .unwrap()
            .reason,
        ReasonCode::StaleMeasurements
    );
    caps.measurements.as_mut().unwrap().valid_until = SystemTime::now() + Duration::from_secs(3600);
    caps.measurements.as_mut().unwrap().maximum_elements = 10_000;
    assert_eq!(
        choose(Policy::Auto, &Workload::checked_i64_map(20_000), &caps)
            .unwrap()
            .reason,
        ReasonCode::MissingMeasurements
    );
}

#[test]
fn explicit_parallel_runs_the_reviewed_map_and_gpu_fails() {
    let caps = measured();
    let input = vec![4; 20_000];
    let execution = execute_map(&input, 3, 7, Policy::ParallelCpu, &caps).unwrap();
    assert_eq!(execution.decision.selected, Backend::ParallelCpu);
    assert_eq!(execution.values, vec![19; 20_000]);
    assert_eq!(
        execute_map(&input, 3, 7, Policy::Gpu, &caps).unwrap_err(),
        cgx2_execution_policy::ExecuteError::Plan(PlanError::UnsupportedBackend {
            requested: Backend::Gpu,
            reason: ReasonCode::GpuUnavailable
        })
    );
}

#[test]
fn overlapping_intervals_do_not_enable_auto_parallelism() {
    let mut caps = measured();
    caps.measurements.as_mut().unwrap().parallel.launch_setup = interval(160_000, 170_000);
    let decision = choose(Policy::Auto, &Workload::checked_i64_map(20_000), &caps).unwrap();
    assert_eq!(
        (decision.selected, decision.reason),
        (Backend::SequentialCpu, ReasonCode::NoMeasuredBenefit)
    );
}

#[test]
fn unsafe_work_is_excluded_even_when_parallel_is_faster() {
    let base = Workload::checked_i64_map(20_000);
    let mut cases = Vec::new();
    let mut w = base.clone();
    w.effects = Effects {
        observable_io: true,
        ..Effects::default()
    };
    cases.push(w);
    let mut w = base.clone();
    w.order_sensitive = true;
    cases.push(w);
    let mut w = base.clone();
    w.shared_mutable_alias = true;
    cases.push(w);
    let mut w = base.clone();
    w.memory_activity = MemoryActivity::ManagedSession;
    cases.push(w);
    let mut w = base.clone();
    w.no_gc = true;
    cases.push(w);
    let mut w = base.clone();
    w.independent = false;
    cases.push(w);
    let mut w = base.clone();
    w.bytes -= 1;
    cases.push(w);
    let mut w = base;
    w.operation = OperationKind::Reduction;
    w.associative = true;
    cases.push(w);
    for w in cases {
        assert_eq!(
            choose(Policy::Auto, &w, &measured()).unwrap().reason,
            ReasonCode::SafetyExclusion
        );
        assert_eq!(
            choose(Policy::ParallelCpu, &w, &measured()).unwrap_err(),
            PlanError::UnsafeWorkload {
                requested: Backend::ParallelCpu,
                reason: ReasonCode::SafetyExclusion
            }
        );
    }
}
