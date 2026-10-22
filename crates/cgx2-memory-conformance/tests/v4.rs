use cgx2_memory_conformance::v3::{Fault, Op};
use cgx2_memory_conformance::v4::{self, Coverage, MODEL_VERSION_V4};

#[test]
fn manifest_and_generator_are_exact_and_bounded() {
    assert_eq!(MODEL_VERSION_V4, 4);
    let seeds = v4::fixed_seeds();
    assert_eq!(seeds, (0..2_000).collect::<Vec<_>>());
    let trace_42 = cgx2_memory_conformance::v3::format_trace(&v4::generate(42));
    let hash = trace_42.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    assert_eq!(hash, 0xfee821be2efde9c0);
    for seed in seeds {
        let trace = v4::generate(seed);
        assert_eq!(trace, v4::generate(seed), "seed={seed}");
        assert!(trace.len() <= 128, "seed={seed}");
        assert!(trace
            .iter()
            .all(|op| op.payload_len().is_none_or(|len| len <= 64)));
        let mut depth = 0;
        let mut regions = 1;
        for op in &trace {
            match op {
                Op::Create { .. } if depth == 0 => regions += 1,
                Op::GuardStart { .. } => {
                    depth += 1;
                    assert!(depth <= 2);
                }
                Op::GuardEnd => {
                    assert!(depth > 0);
                    depth -= 1;
                }
                _ => {}
            }
        }
        assert_eq!(depth, 0);
        assert!(regions <= 4);
    }
}

#[test]
fn named_adversarial_families_and_errors_are_observed() {
    let coverage = v4::run(0, Fault::None).unwrap();
    for family in v4::REQUIRED_FAMILIES {
        assert!(
            coverage.families.get(*family).copied().unwrap_or(0) > 0,
            "{family}"
        );
    }
    for operation in v4::REQUIRED_OPERATIONS {
        assert!(
            coverage.operations.get(*operation).copied().unwrap_or(0) > 0,
            "{operation}"
        );
    }
    for error in v4::REQUIRED_ERRORS {
        assert!(
            coverage.outcomes.get(*error).copied().unwrap_or(0) > 0,
            "{error}"
        );
    }
    assert!(coverage.steps > 0);
    assert!(coverage.skipped > 0);
    assert!(
        coverage
            .skipped_by_operation
            .get("Copy")
            .copied()
            .unwrap_or(0)
            > 0
    );
}

#[test]
fn replay_and_shrink_keep_a_real_per_step_mismatch() {
    let trace = v4::generate(42);
    let first = v4::run_trace(&trace, Fault::OvercountFirstAllocation).unwrap_err();
    assert_eq!(
        first,
        v4::run_trace(&trace, Fault::OvercountFirstAllocation).unwrap_err()
    );
    let small = v4::shrink(&trace, Fault::OvercountFirstAllocation);
    assert!(small.len() < trace.len());
    assert_eq!(small, v4::shrink(&trace, Fault::OvercountFirstAllocation));
    assert!(v4::run_trace(&small, Fault::OvercountFirstAllocation).is_err());
    assert!(!v4::run_trace(&small, Fault::None).is_err());
}

#[test]
fn fixed_corpus_passes_twice_with_identical_summary() {
    let mut first = Coverage::default();
    let mut second = Coverage::default();
    for seed in v4::fixed_seeds() {
        first.merge(&v4::run(seed, Fault::None).unwrap_or_else(|e| panic!("seed={seed}: {e}")));
    }
    for seed in v4::fixed_seeds() {
        second.merge(&v4::run(seed, Fault::None).unwrap_or_else(|e| panic!("seed={seed}: {e}")));
    }
    assert_eq!(first, second);
    assert_eq!(first.seeds, 2_000);
    assert_eq!(first.steps, 254_071);
    assert_eq!(first.skipped, 8_773);
    assert_eq!(first.families.len(), v4::REQUIRED_FAMILIES.len());
    assert!(first.families.values().all(|count| *count == 2_000));
}
