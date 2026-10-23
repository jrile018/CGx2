use cgx2_execution_policy::owned_v1::{self as owned, *};
use cgx2_execution_policy::profile::{MachineFingerprint, ProfileError};
use cgx2_execution_policy::{Backend, ExecuteError, MapError, Policy};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

fn machine() -> MachineFingerprint {
    MachineFingerprint {
        os: "linux".into(),
        arch: "x86_64".into(),
        cpu: "fixture-only".into(),
        available_threads: 4,
        rustc: "fixture-rustc".into(),
    }
}
fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1000)
}
fn input(operation: Operation, size: usize) -> Input {
    match operation {
        Operation::Map => Input::Map {
            values: vec![2; size],
            multiplier: 3,
            addend: 7,
        },
        Operation::ZipMap => Input::ZipMap {
            left: vec![2; size],
            right: vec![3; size],
            addend: 7,
        },
    }
}
fn raw(operation: Operation) -> String {
    // Deliberately synthetic loader fixtures, never benchmark evidence.
    let mut csv = "schema,phase,kind,elements,workers,sample,elapsed_ns,os,arch,cpu,available_threads,rustc\n".to_string();
    for phase in ["calibration", "holdout"] {
        for sample in 0..3 {
            for (kind, elapsed) in [("sequential", 1000 + sample), ("parallel", 100 + sample)] {
                csv.push_str(&format!("{},{phase},{kind},4096,4,{sample},{elapsed},linux,x86_64,fixture-only,4,fixture-rustc\n", operation.identity().0));
            }
        }
    }
    csv
}
fn profile(operation: Operation) -> (String, String) {
    let csv = raw(operation);
    let p = owned::build_profile(
        operation,
        &csv,
        &machine(),
        4,
        4096,
        now(),
        Duration::from_secs(100),
    )
    .unwrap();
    (p.to_text(), csv)
}
fn map_error(error: MapError) -> Error {
    Error::Execution(ExecuteError::Map(error))
}

#[test]
fn owned_maps_match_checked_reference_at_integer_and_chunk_boundaries() {
    let machine = machine();
    let context = Context {
        parallel_workers: 2,
        machine: &machine,
        profile: None,
    };
    let flag = AtomicBool::new(false);
    for size in [0, 1, 2047, 2048, 2049, 3071, 3072, 4099, 8193] {
        for operation in [Operation::Map, Operation::ZipMap] {
            for policy in [Policy::Auto, Policy::SequentialCpu, Policy::ParallelCpu] {
                let outcome = execute_at(
                    input(operation, size),
                    policy,
                    context,
                    &flag,
                    Budgets::default(),
                    now(),
                );
                if policy == Policy::ParallelCpu && size < 2048 {
                    assert_eq!(
                        outcome.result,
                        Err(map_error(MapError::InvalidWorkers { requested: 2 }))
                    );
                } else {
                    assert_eq!(outcome.result, Ok(vec![13; size]));
                    assert_eq!(outcome.report.schema_version, 1);
                    assert_eq!(outcome.report.operation, operation);
                    assert_eq!(outcome.report.elements, size);
                    if policy == Policy::Auto {
                        assert_eq!(
                            outcome.report.decision.unwrap().selected,
                            Backend::SequentialCpu
                        );
                    }
                }
            }
        }
    }
    for (a, b, c) in [
        (i64::MIN, -1, i64::MIN),
        (i64::MAX, 2, -1),
        (i64::MAX, 1, 1),
        (i64::MIN, 1, -1),
        (0, i64::MIN, i64::MAX),
        (i64::MIN, 1, 0),
    ] {
        let expected = a.checked_mul(b).and_then(|v| v.checked_add(c));
        for policy in [Policy::SequentialCpu, Policy::ParallelCpu] {
            for operation in [Operation::Map, Operation::ZipMap] {
                let mut values = vec![0; 4099];
                values[1025] = a;
                values[4098] = a;
                let data = match operation {
                    Operation::Map => Input::Map {
                        values,
                        multiplier: b,
                        addend: c,
                    },
                    Operation::ZipMap => Input::ZipMap {
                        left: values,
                        right: vec![b; 4099],
                        addend: c,
                    },
                };
                let outcome = execute_at(data, policy, context, &flag, Budgets::default(), now());
                match expected {
                    Some(v) => assert_eq!(outcome.result.unwrap()[1025], v),
                    None => assert_eq!(
                        outcome.result,
                        Err(map_error(MapError::Overflow { index: 1025 }))
                    ),
                }
            }
        }
    }
}

#[test]
fn all_owned_backends_validate_before_cancellation_and_never_publish_partial_output() {
    let machine = machine();
    let context = Context {
        parallel_workers: 2,
        machine: &machine,
        profile: None,
    };
    let flag = AtomicBool::new(true);
    for operation in [Operation::Map, Operation::ZipMap] {
        for policy in [Policy::Auto, Policy::SequentialCpu, Policy::ParallelCpu] {
            let out = execute_at(
                input(operation, 4099),
                policy,
                context,
                &flag,
                Budgets::default(),
                now(),
            );
            assert_eq!(out.result, Err(map_error(MapError::Cancelled)));
        }
        for workers in [0, 1, 5, 33, usize::MAX] {
            let out = execute_at(
                input(operation, 4099),
                Policy::ParallelCpu,
                Context {
                    parallel_workers: workers,
                    ..context
                },
                &flag,
                Budgets::default(),
                now(),
            );
            assert_eq!(
                out.result,
                Err(map_error(MapError::InvalidWorkers { requested: workers }))
            );
        }
        let gpu = execute_at(
            input(operation, 4099),
            Policy::Gpu,
            context,
            &flag,
            Budgets {
                requested_buffer_bytes: Some(0),
                ..Budgets::default()
            },
            now(),
        );
        assert!(matches!(
            gpu.result,
            Err(Error::Execution(ExecuteError::Plan(_)))
        ));
        assert!(gpu.report.decision.is_none());
        assert_eq!(gpu.report.buffer_budget, BudgetStatus::NotEvaluated);
    }
    let out = execute_at(
        Input::ZipMap {
            left: vec![1],
            right: vec![],
            addend: 0,
        },
        Policy::ParallelCpu,
        context,
        &flag,
        Budgets::default(),
        now(),
    );
    assert_eq!(
        out.result,
        Err(map_error(MapError::LengthMismatch { left: 1, right: 0 }))
    );
}

#[test]
fn requested_buffers_are_bounded_and_latency_violations_are_observations() {
    let machine = machine();
    let context = Context {
        parallel_workers: 2,
        machine: &machine,
        profile: None,
    };
    let flag = AtomicBool::new(false);
    for operation in [Operation::Map, Operation::ZipMap] {
        for policy in [Policy::SequentialCpu, Policy::ParallelCpu] {
            let out = execute_at(
                input(operation, 4096),
                policy,
                context,
                &flag,
                Budgets::default(),
                now(),
            );
            let buffers = out.report.buffers.unwrap();
            let count = 2
                + u128::from(operation == Operation::ZipMap)
                + u128::from(policy == Policy::ParallelCpu);
            assert_eq!(buffers.peak_requested_bytes, 4096 * 8 * count);
            let required = buffers.peak_requested_bytes;
            let out = execute_at(
                input(operation, 4096),
                policy,
                context,
                &flag,
                Budgets {
                    requested_buffer_bytes: Some(required - 1),
                    elapsed_ns: Some(u128::MAX),
                },
                now(),
            );
            assert_eq!(
                out.result,
                Err(Error::BufferBudgetExceeded {
                    required,
                    limit: required - 1
                })
            );
            assert_eq!(out.report.buffer_budget, BudgetStatus::Exceeded);
            assert_eq!(out.report.elapsed_budget, BudgetStatus::Within);
            let out = execute_at(
                input(operation, 4096),
                policy,
                context,
                &flag,
                Budgets {
                    requested_buffer_bytes: Some(required),
                    elapsed_ns: Some(0),
                },
                now(),
            );
            assert!(out.result.is_ok());
            assert_eq!(out.report.buffer_budget, BudgetStatus::Within);
            assert_eq!(out.report.elapsed_budget, BudgetStatus::Exceeded);
        }
    }
    let values = Vec::with_capacity(128);
    let out = execute_at(
        Input::Map {
            values,
            multiplier: 1,
            addend: 0,
        },
        Policy::Auto,
        context,
        &flag,
        Budgets {
            requested_buffer_bytes: Some(1023),
            ..Budgets::default()
        },
        now(),
    );
    assert_eq!(
        out.result,
        Err(Error::BufferBudgetExceeded {
            required: 1024,
            limit: 1023
        })
    );
}

#[test]
fn profile_evidence_is_opaque_operation_specific_and_revalidated_per_dispatch() {
    let machine = machine();
    let flag = AtomicBool::new(false);
    for operation in [Operation::Map, Operation::ZipMap] {
        let (text, csv) = profile(operation);
        let validated = ValidatedProfile::load(operation, &text, &csv, &machine, 4, now()).unwrap();
        let context = Context {
            parallel_workers: 4,
            machine: &machine,
            profile: Some(&validated),
        };
        for policy in [Policy::Auto, Policy::SequentialCpu, Policy::ParallelCpu] {
            let out = execute_at(
                input(operation, 4096),
                policy,
                context,
                &flag,
                Budgets::default(),
                now(),
            );
            assert_eq!(out.result, Ok(vec![13; 4096]));
            assert!(matches!(
                out.report.profile,
                ProfileStatus::Accepted {
                    calibration_samples: 3,
                    heldout_samples: 3,
                    ..
                }
            ));
            if policy == Policy::Auto {
                assert_eq!(out.report.decision.unwrap().selected, Backend::ParallelCpu);
            }
        }
        let other = if operation == Operation::Map {
            Operation::ZipMap
        } else {
            Operation::Map
        };
        let mut wrong_machine = machine.clone();
        wrong_machine.cpu = "other-host".into();
        for (op, size, ctx, time, reason) in [
            (other, 4096, context, now(), ProfileRejection::Operation),
            (operation, 4097, context, now(), ProfileRejection::Size),
            (
                operation,
                4096,
                Context {
                    parallel_workers: 2,
                    ..context
                },
                now(),
                ProfileRejection::Workers,
            ),
            (
                operation,
                4096,
                Context {
                    machine: &wrong_machine,
                    ..context
                },
                now(),
                ProfileRejection::Machine,
            ),
            (
                operation,
                4096,
                context,
                now() + Duration::from_secs(100),
                ProfileRejection::Age,
            ),
            (
                operation,
                4096,
                context,
                now() - Duration::from_secs(1),
                ProfileRejection::Age,
            ),
        ] {
            let out = execute_at(
                input(op, size),
                Policy::Auto,
                ctx,
                &flag,
                Budgets::default(),
                time,
            );
            assert!(out.result.is_ok());
            assert_eq!(out.report.profile, ProfileStatus::Rejected(reason));
            assert_eq!(
                out.report.decision.unwrap().selected,
                Backend::SequentialCpu
            );
        }
        assert!(ValidatedProfile::load(other, &text, &csv, &machine, 4, now()).is_err());
        assert!(ValidatedProfile::load(operation, &text, &csv, &wrong_machine, 4, now()).is_err());
        assert!(ValidatedProfile::load(operation, &text, &csv, &machine, 2, now()).is_err());
        assert!(
            ValidatedProfile::load(
                operation,
                &text,
                &csv,
                &machine,
                4,
                now() + Duration::from_secs(100)
            )
            .is_err()
        );
        assert!(
            ValidatedProfile::load(
                operation,
                &text.replace("seq_lower_ns=1000", "seq_lower_ns=1"),
                &csv,
                &machine,
                4,
                now()
            )
            .is_err()
        );
        assert!(
            ValidatedProfile::load(
                operation,
                &text,
                &csv.replace(",1000,", ",1001,"),
                &machine,
                4,
                now()
            )
            .is_err()
        );
        let noisy = csv.replace(",102,", ",999,");
        assert_eq!(
            owned::build_profile(
                operation,
                &noisy,
                &machine,
                4,
                4096,
                now(),
                Duration::from_secs(100)
            ),
            Err(ProfileError::NoisySamples)
        );
        let incomplete = csv
            .lines()
            .filter(|s| !s.contains("holdout,parallel,4096,4,2,"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            owned::build_profile(
                operation,
                &incomplete,
                &machine,
                4,
                4096,
                now(),
                Duration::from_secs(100)
            ),
            Err(ProfileError::InsufficientSamples)
        );
        for ttl in [Duration::ZERO, Duration::from_secs(604801)] {
            assert!(owned::build_profile(operation, &csv, &machine, 4, 4096, now(), ttl).is_err());
        }
    }
}

#[test]
fn borrowed_cancellable_scalar_api_preserves_validation_and_values() {
    use cgx2_execution_policy::{Capabilities, execute_map_cancellable};
    let caps = Capabilities {
        parallel_workers: 2,
        ..Capabilities::default()
    };
    for policy in [Policy::Auto, Policy::SequentialCpu, Policy::ParallelCpu] {
        let out =
            execute_map_cancellable(&vec![2; 4099], 3, 7, policy, &caps, &AtomicBool::new(false))
                .unwrap();
        assert_eq!(out.values, vec![13; 4099]);
        assert_eq!(
            execute_map_cancellable(&vec![2; 4099], 3, 7, policy, &caps, &AtomicBool::new(true)),
            Err(ExecuteError::Map(MapError::Cancelled))
        );
    }
    assert_eq!(
        execute_map_cancellable(
            &[],
            3,
            7,
            Policy::ParallelCpu,
            &caps,
            &AtomicBool::new(true)
        ),
        Err(ExecuteError::Map(MapError::InvalidWorkers { requested: 2 }))
    );
}

#[test]
fn uncertain_or_borrowed_evidence_cannot_enable_owned_auto() {
    let machine = machine();
    let flag = AtomicBool::new(false);
    for operation in [Operation::Map, Operation::ZipMap] {
        let csv = raw(operation)
            .replace(",100,", ",900,")
            .replace(",101,", ",1100,")
            .replace(",102,", ",900,");
        let saved = owned::build_profile(
            operation,
            &csv,
            &machine,
            4,
            4096,
            now(),
            Duration::from_secs(100),
        )
        .unwrap();
        let profile =
            ValidatedProfile::load(operation, &saved.to_text(), &csv, &machine, 4, now()).unwrap();
        let out = execute_at(
            input(operation, 4096),
            Policy::Auto,
            Context {
                parallel_workers: 4,
                machine: &machine,
                profile: Some(&profile),
            },
            &flag,
            Budgets::default(),
            now(),
        );
        assert_eq!(
            out.report.decision.unwrap().reason,
            cgx2_execution_policy::ReasonCode::NoMeasuredBenefit
        );
        assert_eq!(out.report.measured_costs.unwrap().parallel.upper_ns, 1210);
        assert_eq!(out.report.machine, machine);
        let old_schema = if operation == Operation::Map { 1 } else { 2 };
        let legacy_csv = raw(operation).replace(
            &format!("\n{},", operation.identity().0),
            &format!("\n{old_schema},"),
        );
        let legacy = if operation == Operation::Map {
            cgx2_execution_policy::profile::build_profile(
                &legacy_csv,
                &machine,
                4,
                now(),
                Duration::from_secs(100),
            )
        } else {
            cgx2_execution_policy::profile::build_zip_profile(
                &legacy_csv,
                &machine,
                4,
                now(),
                Duration::from_secs(100),
            )
        }
        .unwrap();
        assert!(
            ValidatedProfile::load(
                operation,
                &legacy.to_text(),
                &legacy_csv,
                &machine,
                4,
                now()
            )
            .is_err()
        );
        let huge = raw(operation)
            .replace(",1000,", &format!(",{},", u128::MAX))
            .replace(",1001,", &format!(",{},", u128::MAX))
            .replace(",1002,", &format!(",{},", u128::MAX));
        let invalid = owned::build_profile(
            operation,
            &huge,
            &machine,
            4,
            4096,
            now(),
            Duration::from_secs(100),
        );
        assert_eq!(invalid, Err(ProfileError::InvalidProfile));
    }
}
