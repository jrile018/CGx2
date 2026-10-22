use cgx2_memory_conformance::v5::{generate, run, run_trace, shrink, Op};
use cgx2_runtime::{AllocationError, CollectionMode, MemoryError, Session, SLOT_METADATA_BYTES};

#[test]
fn independent_preflight_and_reclaim_transitions() {
    for seed in [0, 1, 42, 255] {
        let trace = generate(seed);
        // The fixed prefix includes an impossible request, an intrinsically
        // over-limit request, a threshold collection, and a rooted failure.
        let prefix = &trace[..9];
        let coverage = run_trace(prefix, false).unwrap();
        assert_eq!(coverage.steps, 9);
        assert_eq!(coverage.errors["Err(HostAllocation)"], 1);
        assert_eq!(coverage.errors["Err(RegionLimit)"], 2);
        assert_eq!(coverage.errors["Err(InvalidHandle)"], 1);
    }
}

#[test]
fn deterministic_v5_corpus_checks_every_state() {
    for seed in 0..64 {
        let coverage = run(seed).unwrap();
        assert!(coverage.steps >= 90, "seed={seed}");
        assert!(coverage.errors.contains_key("Err(CollectionForbidden)"));
        assert!(coverage.errors.contains_key("Err(BudgetExceeded)"));
        assert!(coverage.errors.contains_key("Err(RegionHasRoots)"));
        assert_eq!(coverage, run(seed).unwrap(), "seed={seed}");
    }
}

#[test]
fn reducer_replays_a_real_step_mismatch_in_a_fresh_session() {
    let trace = generate(42);
    let reduced = shrink(&trace, true);
    assert!(reduced.len() < trace.len());
    assert!(run_trace(&reduced, true).unwrap_err().starts_with("step "));
    assert!(run_trace(&reduced, false).is_ok());
}

#[test]
fn unwinding_nested_guards_restores_collection_permission() {
    let mut session = Session::new();
    let charge = SLOT_METADATA_BYTES + 1;
    let region = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: charge,
            },
            charge,
        )
        .unwrap();
    let old = session.alloc_bytes(region, 1).unwrap();
    let before = session.stats(region).unwrap();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut outer = session.no_gc(charge);
        let mut inner = outer.no_gc(0);
        assert_eq!(
            inner.alloc_bytes(region, 1),
            Err(AllocationError::BudgetExceeded)
        );
        assert_eq!(
            inner.alloc_bytes(region, usize::MAX),
            Err(AllocationError::HostAllocation)
        );
        assert_eq!(inner.collect(region), Err(MemoryError::CollectionForbidden));
        assert_eq!(
            inner.stats(region).unwrap().collection_count,
            before.collection_count
        );
        panic!("cancel the protected operation");
    }));
    assert!(unwind.is_err());
    assert_eq!(
        session.stats(region).unwrap().collection_count,
        before.collection_count
    );
    let replacement = session.alloc_bytes(region, 1).unwrap();
    assert_eq!(
        session.stats(region).unwrap().collection_count,
        before.collection_count + 1
    );
    assert_eq!(session.read_bytes(old), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(replacement), Ok(&[0][..]));
}

#[test]
fn v5_trace_has_bounded_fixed_prefix_and_replayable_ops() {
    let trace = generate(0);
    assert!(trace.len() <= 96);
    assert!(matches!(
        trace[2],
        Op::Alloc {
            len: usize::MAX,
            ..
        }
    ));
    assert!(matches!(trace[15], Op::GuardStart { .. }));
    assert!(matches!(trace[35], Op::Link { .. }));
}
