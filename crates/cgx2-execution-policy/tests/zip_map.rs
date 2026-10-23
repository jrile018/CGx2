use cgx2_execution_policy::{
    Backend, Capabilities, ExecuteError, MapError, Policy, ReasonCode, execute_zip_map,
    zip_map_i64_checked_parallel, zip_map_i64_checked_sequential,
};

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
        let reference = zip_map_i64_checked_sequential(&left, &right, addend);
        for workers in [2, 3, 4, 8] {
            let result = zip_map_i64_checked_parallel(&left, &right, addend, workers);
            if size >= 2048 && workers <= size / 1024 {
                assert_eq!(result, reference, "case={case} workers={workers}");
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
