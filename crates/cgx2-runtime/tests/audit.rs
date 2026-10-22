use std::panic::{catch_unwind, AssertUnwindSafe};

use cgx2_runtime::{AllocationError, CollectionMode, MemoryError, Session, SLOT_METADATA_BYTES};

#[test]
fn unwinding_nested_guards_does_not_collect_or_leave_a_budget_active() {
    let mut session = Session::new();
    let auto = session
        .create_region(CollectionMode::Auto { trigger_bytes: 1 }, 256)
        .unwrap();
    let manual = session.create_region(CollectionMode::Manual, 256).unwrap();
    let mut retained = None;
    let unwind = catch_unwind(AssertUnwindSafe(|| {
        let mut outer = session.no_gc(SLOT_METADATA_BYTES);
        retained = Some(outer.alloc_bytes(auto, 0).unwrap());
        let mut inner = outer.no_gc(0);
        assert_eq!(
            inner.alloc_bytes(manual, 0),
            Err(AllocationError::BudgetExceeded)
        );
        assert_eq!(inner.collect(auto), Err(MemoryError::CollectionForbidden));
        panic!("exercise guard drop during unwind");
    }));
    assert!(unwind.is_err());
    let before = session.memory_report();
    assert_eq!(before.regions[1].committed_bytes, SLOT_METADATA_BYTES);
    assert_eq!(before.regions[1].collection_count, 0);
    assert_eq!(before.regions[2].committed_bytes, 0);
    assert_eq!(before.regions[2].collection_count, 0);
    assert_eq!(before.regions[2].allocation_failures, 1);

    let fresh = session.alloc_bytes(auto, 0).unwrap();
    assert_eq!(
        session.read_bytes(retained.unwrap()),
        Err(MemoryError::InvalidHandle)
    );
    assert!(session.read_bytes(fresh).is_ok());
    assert_eq!(session.stats(auto).unwrap().collection_count, 1);
    assert_eq!(session.stats(manual).unwrap().collection_count, 0);
}

#[test]
fn closed_region_id_and_handles_remain_stale_after_new_region_is_created() {
    let mut session = Session::new();
    let closed = session.create_region(CollectionMode::Manual, 256).unwrap();
    let other = session.create_region(CollectionMode::Manual, 256).unwrap();
    let old = session.alloc_bytes(closed, 1).unwrap();
    let survivor = session.alloc_bytes(other, 1).unwrap();
    session.root(survivor).unwrap();
    session.close_region(closed).unwrap();
    let replacement = session.create_region(CollectionMode::Manual, 256).unwrap();
    let current = session.alloc_bytes(replacement, 1).unwrap();
    assert_ne!(replacement, closed);
    assert_eq!(
        session.alloc_bytes(closed, 0),
        Err(AllocationError::InvalidRegion)
    );
    assert_eq!(session.read_bytes(old), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(survivor).unwrap(), &[0]);
    assert_eq!(session.read_bytes(current).unwrap(), &[0]);
    assert_eq!(session.stats(other).unwrap().collection_count, 0);
    assert_eq!(session.memory_report().regions[1].committed_bytes, 0);
}
