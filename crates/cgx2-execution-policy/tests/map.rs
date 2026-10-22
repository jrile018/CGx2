use cgx2_execution_policy::{MapError, map_i64_checked_parallel, map_i64_checked_sequential};

#[test]
fn scalar_parallel_equivalence_on_sizes_and_workers() {
    for size in [0, 1, 2, 1023, 1024, 2048, 4097, 40_000] {
        let input: Vec<i64> = (0..size).map(|i| (i as i64 % 97) - 48).collect();
        let expected = map_i64_checked_sequential(&input, -3, 19);
        for workers in [1, 2, 3, 4, 32, 100_000] {
            assert_eq!(
                map_i64_checked_parallel(&input, -3, 19, workers),
                expected,
                "size={size} workers={workers}"
            );
        }
    }
}

#[test]
fn first_overflow_is_in_source_order() {
    let mut input = vec![1; 10_000];
    input[2049] = i64::MAX;
    input[8192] = i64::MIN;
    for workers in [1, 2, 3, 8, 32] {
        assert_eq!(
            map_i64_checked_parallel(&input, 2, 0, workers),
            Err(MapError::Overflow { index: 2049 })
        );
    }
    assert_eq!(
        map_i64_checked_sequential(&input, 2, 0),
        Err(MapError::Overflow { index: 2049 })
    );
    assert_eq!(
        map_i64_checked_parallel(&[i64::MAX], 1, 1, 8),
        Err(MapError::Overflow { index: 0 })
    );
}
