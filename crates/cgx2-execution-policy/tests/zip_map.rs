use cgx2_execution_policy::{
    Backend, Capabilities, ExecuteError, MapError, Policy, ReasonCode, execute_zip_map,
    execute_zip_map_cancellable, map_i64_checked_parallel, map_i64_checked_sequential,
    zip_map_i64_checked_parallel, zip_map_i64_checked_parallel_cancellable,
    zip_map_i64_checked_sequential,
};
use std::sync::atomic::AtomicBool;

fn next(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed
}

#[test]
fn bounded_seeded_corpus_matches_every_backend() {
    let mut seed = 0x5eedead_u64;
    for case in 0..96 {
        let size = match case % 8 {
            0 => 0,
            1 => 1,
            2 => 17,
            3 => 2047,
            4 => 2048,
            5 => 4096,
            6 => 8193,
            _ => (next(&mut seed) as usize % 9000) + 1,
        };
        let value = |seed: &mut u64| match next(seed) % 12 {
            0 => i64::MIN,
            1 => i64::MAX,
            2 => -1,
            3 => 0,
            4 => 1,
            _ => (next(seed) % 201) as i64 - 100,
        };
        let left: Vec<_> = (0..size).map(|_| value(&mut seed)).collect();
        let right: Vec<_> = (0..size).map(|_| value(&mut seed)).collect();
        let addend = value(&mut seed);
        let multiplier = value(&mut seed);
        let reference = zip_map_i64_checked_sequential(&left, &right, addend);
        let map_reference = map_i64_checked_sequential(&left, multiplier, addend);
        for workers in [2, 3, 4, 8] {
            assert_eq!(
                map_i64_checked_parallel(&left, multiplier, addend, workers),
                map_reference,
                "map seed=0x5eedead case={case} workers={workers}"
            );
            let result = zip_map_i64_checked_parallel(&left, &right, addend, workers);
            if size >= 2048 && workers <= size / 1024 {
                assert_eq!(
                    result, reference,
                    "zip seed=0x5eedead case={case} workers={workers}"
                );
                let caps = Capabilities {
                    parallel_workers: workers,
                    ..Capabilities::default()
                };
                assert_eq!(
                    execute_zip_map(&left, &right, addend, Policy::ParallelCpu, &caps)
                        .map(|execution| execution.values),
                    reference.clone().map_err(ExecuteError::Map),
                    "case={case} workers={workers}"
                );
            } else {
                assert_eq!(result, Err(MapError::InvalidWorkers { requested: workers }));
            }
        }
    }
}

#[test]
fn mismatches_workers_and_first_error_are_deterministic() {
    assert_eq!(
        zip_map_i64_checked_sequential(&[i64::MAX], &[1], 1),
        Err(MapError::Overflow { index: 0 })
    );
    assert_eq!(
        zip_map_i64_checked_sequential(&[i64::MIN], &[-1], 0),
        Err(MapError::Overflow { index: 0 })
    );
    assert_eq!(
        zip_map_i64_checked_sequential(&[1], &[], 0),
        Err(MapError::LengthMismatch { left: 1, right: 0 })
    );
    assert_eq!(
        zip_map_i64_checked_parallel(&[1], &[], 0, 2),
        Err(MapError::LengthMismatch { left: 1, right: 0 })
    );
    assert_eq!(
        execute_zip_map(&[1], &[], 0, Policy::Auto, &Capabilities::default()),
        Err(ExecuteError::Map(MapError::LengthMismatch {
            left: 1,
            right: 0
        }))
    );
    let mut left = vec![2; 4096];
    let right = vec![2; 4096];
    left[7] = i64::MAX;
    left[2050] = i64::MAX;
    for workers in [2, 3, 4] {
        assert_eq!(
            zip_map_i64_checked_parallel(&left, &right, 0, workers),
            Err(MapError::Overflow { index: 7 })
        );
    }
    for workers in [0, 1, 33] {
        assert_eq!(
            zip_map_i64_checked_parallel(&left, &right, 0, workers),
            Err(MapError::InvalidWorkers { requested: workers })
        );
    }
    assert_eq!(
        zip_map_i64_checked_parallel(&vec![1; 32_768], &vec![2; 32_768], 3, 32),
        Ok(vec![5; 32_768])
    );
    let caps = Capabilities {
        parallel_workers: 33,
        ..Capabilities::default()
    };
    assert_eq!(
        execute_zip_map(&left, &right, 0, Policy::ParallelCpu, &caps),
        Err(ExecuteError::Map(MapError::InvalidWorkers {
            requested: 33
        }))
    );
    let caps = Capabilities {
        parallel_workers: 8,
        ..Capabilities::default()
    };
    assert_eq!(
        execute_zip_map(&left, &right, 0, Policy::ParallelCpu, &caps),
        Err(ExecuteError::Map(MapError::InvalidWorkers { requested: 8 }))
    );
}

#[test]
fn uncalibrated_auto_is_sequential_and_gpu_is_unsupported() {
    let left = vec![3; 4096];
    let right = vec![4; 4096];
    let caps = Capabilities {
        parallel_workers: 4,
        ..Capabilities::default()
    };
    let execution = execute_zip_map(&left, &right, 7, Policy::Auto, &caps).unwrap();
    assert_eq!(execution.values, vec![19; 4096]);
    assert_eq!(execution.decision.selected, Backend::SequentialCpu);
    assert_eq!(execution.decision.reason, ReasonCode::MissingMeasurements);
    assert!(matches!(
        execute_zip_map(&left, &right, 7, Policy::Gpu, &caps),
        Err(ExecuteError::Plan(_))
    ));
}

#[test]
fn requested_cancellation_and_validation_precedence() {
    let cancelled = AtomicBool::new(true);
    let left = vec![2; 4_099];
    let right = vec![3; left.len()];
    assert_eq!(
        zip_map_i64_checked_parallel_cancellable(&left, &right, 7, 3, &cancelled),
        Err(MapError::Cancelled)
    );
    assert_eq!(
        zip_map_i64_checked_parallel_cancellable(&left, &right[..1], 7, 3, &cancelled),
        Err(MapError::LengthMismatch {
            left: left.len(),
            right: 1
        })
    );
    assert_eq!(
        zip_map_i64_checked_parallel_cancellable(&left, &right, 7, 0, &cancelled),
        Err(MapError::InvalidWorkers { requested: 0 })
    );
    for policy in [Policy::SequentialCpu, Policy::ParallelCpu, Policy::Auto] {
        let caps = Capabilities {
            parallel_workers: 3,
            ..Capabilities::default()
        };
        assert_eq!(
            execute_zip_map_cancellable(&left, &right, 7, policy, &caps, &cancelled),
            Err(ExecuteError::Map(MapError::Cancelled))
        );
    }
}

#[test]
fn exact_chunk_boundaries_and_both_pure_maps_match() {
    for size in [0, 1, 2_047, 2_048, 2_049, 3_071, 3_072, 4_099, 8_193] {
        let mut left = vec![2; size];
        let right = vec![3; size];
        let expected = zip_map_i64_checked_sequential(&left, &right, 7);
        assert_eq!(expected, Ok(vec![13; size]));
        let map_expected = map_i64_checked_sequential(&left, 3, 7);
        for workers in [2, 3, 4, 8] {
            assert_eq!(
                map_i64_checked_parallel(&left, 3, 7, workers),
                map_expected,
                "map size={size} workers={workers}"
            );
            let result = zip_map_i64_checked_parallel(&left, &right, 7, workers);
            if size >= 2_048 && workers <= size / 1_024 {
                assert_eq!(result, expected, "zip size={size} workers={workers}");
            } else {
                assert_eq!(result, Err(MapError::InvalidWorkers { requested: workers }));
            }
        }
        if size >= 2_048 {
            for index in [0, size / 2, size - 1] {
                left[index] = i64::MAX;
                let overflow = Err(MapError::Overflow { index });
                assert_eq!(zip_map_i64_checked_sequential(&left, &right, 7), overflow);
                for workers in [2, 3, 4, 8] {
                    if workers <= size / 1_024 {
                        assert_eq!(
                            zip_map_i64_checked_parallel(&left, &right, 7, workers),
                            overflow,
                            "overflow size={size} workers={workers} index={index}"
                        );
                    }
                }
                left[index] = 2;
            }
        }
    }
}
