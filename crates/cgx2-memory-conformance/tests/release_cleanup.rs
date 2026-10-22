//! Failure and cancellation cleanup for the direct runtime release packet.
use cgx2_runtime::{
    AllocationError, CollectionMode, MemoryError, Session, REFERENCE_SLOT_METADATA_BYTES,
    SLOT_METADATA_BYTES,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

const CHARGE: usize = SLOT_METADATA_BYTES + 1 + REFERENCE_SLOT_METADATA_BYTES;

#[test]
fn unwind_and_returned_error_release_nested_guards_without_implicit_collection() {
    let mut session = Session::new();
    let auto = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: 2 * CHARGE,
            },
            8 * CHARGE,
        )
        .unwrap();
    let manual = session
        .create_region(CollectionMode::Manual, 8 * CHARGE)
        .unwrap();
    let parent = session.alloc_object(auto, 1, 1).unwrap();
    let child = session.alloc_object(auto, 1, 1).unwrap();
    let manual_root = session.alloc_object(manual, 1, 1).unwrap();
    session.root(parent).unwrap();
    session.root(manual_root).unwrap();
    session.set_reference(parent, 0, Some(child)).unwrap();
    assert_eq!(
        session.set_reference(parent, 0, Some(manual_root)),
        Err(MemoryError::CrossRegionEdge)
    );
    assert_eq!(session.reference(parent, 0), Ok(Some(child)));

    let before = session.memory_report().totals.collection_count;
    let cancelled = catch_unwind(AssertUnwindSafe(|| {
        let mut outer = session.no_gc(2 * CHARGE);
        let mut inner = outer.no_gc(CHARGE);
        inner.inject_host_failure_at_reserve(1);
        assert_eq!(
            inner.alloc_object(auto, 1, 1),
            Err(AllocationError::HostAllocation)
        );
        assert_eq!(inner.memory_report().totals.collection_count, before);
        let pending = inner.alloc_object(auto, 1, 1).unwrap();
        assert_eq!(inner.read_bytes(pending), Ok(&[0][..]));
        assert_eq!(
            inner.alloc_object(manual, 1, 1),
            Err(AllocationError::BudgetExceeded)
        );
        assert_eq!(inner.collect(manual), Err(MemoryError::CollectionForbidden));
        assert_eq!(inner.memory_report().totals.collection_count, before);
        panic!("cancel managed work");
    }));
    assert!(cancelled.is_err());
    assert_eq!(session.memory_report().totals.collection_count, before);
    assert_eq!(session.reference(parent, 0), Ok(Some(child)));
    assert_eq!(session.read_bytes(manual_root), Ok(&[0][..]));

    // Both guards were dropped during unwind. The next auto allocation is
    // a safepoint; its collection retains the rooted graph.
    let after_cancel = session.alloc_object(auto, 1, 1).unwrap();
    assert_eq!(session.stats(auto).unwrap().collection_count, 1);
    assert_eq!(session.stats(manual).unwrap().collection_count, 0);
    assert_eq!(session.reference(parent, 0), Ok(Some(child)));
    assert_eq!(session.read_bytes(after_cancel), Ok(&[0][..]));

    // A propagated allocation error also drops the guard and leaves manual
    // collection entirely explicit. The earlier successful allocation stays.
    let manual_before = session.stats(manual).unwrap();
    let failed: Result<(), AllocationError> = (|| {
        let mut guard = session.no_gc(CHARGE);
        guard.alloc_object(manual, 1, 1)?;
        guard.alloc_object(manual, 1, 1)?;
        Ok(())
    })();
    assert_eq!(failed, Err(AllocationError::BudgetExceeded));
    let manual_after = session.stats(manual).unwrap();
    assert_eq!(
        manual_after.collection_count,
        manual_before.collection_count
    );
    assert_eq!(
        manual_after.committed_bytes,
        manual_before.committed_bytes + CHARGE
    );
    assert_eq!(
        manual_after.allocation_failures,
        manual_before.allocation_failures + 1
    );
    let retry = session.alloc_object(manual, 1, 1).unwrap();
    assert_eq!(session.stats(manual).unwrap().collection_count, 0);
    assert_eq!(session.read_bytes(retry), Ok(&[0][..]));

    assert_eq!(
        session.close_region(manual),
        Err(MemoryError::RegionHasRoots)
    );
    session.unroot(manual_root).unwrap();
    session.collect(manual).unwrap();
    assert_eq!(session.stats(manual).unwrap().committed_bytes, 0);
    session.close_region(manual).unwrap();
    assert_eq!(
        session.read_bytes(manual_root),
        Err(MemoryError::InvalidHandle)
    );
    assert_eq!(session.reference(parent, 0), Ok(Some(child)));

    session.unroot(parent).unwrap();
    session.collect(auto).unwrap();
    assert_eq!(session.stats(auto).unwrap().committed_bytes, 0);
    session.close_region(auto).unwrap();
    assert_eq!(session.read_bytes(parent), Err(MemoryError::InvalidHandle));
}
