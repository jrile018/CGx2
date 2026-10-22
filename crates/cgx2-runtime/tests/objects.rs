use cgx2_runtime::{
    AllocationError, CollectionMode, MemoryError, Session, MAX_OBJECT_PAYLOAD_BYTES,
    MAX_OBJECT_REFERENCE_SLOTS, REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES,
};

#[test]
fn checked_slots_trace_shared_children_and_reclaim_cycles_after_clear() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let a = session.alloc_object(region, 2, 2).unwrap();
    let b = session.alloc_object(region, 3, 1).unwrap();
    let shared = session.alloc_object(region, 1, 0).unwrap();
    session.set_reference(a, 0, Some(b)).unwrap();
    session.set_reference(b, 0, Some(a)).unwrap();
    session.set_reference(a, 1, Some(shared)).unwrap();
    session.set_reference(b, 0, Some(shared)).unwrap();
    session.root(a).unwrap();
    session.collect(region).unwrap();
    assert!(session.read_bytes(shared).is_ok());
    assert!(session.read_bytes(b).is_ok());

    session.set_reference(a, 0, None).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(b), Err(MemoryError::InvalidHandle));
    session.set_reference(a, 0, Some(shared)).unwrap();
    session.set_reference(a, 1, None).unwrap();
    session.collect(region).unwrap();
    assert!(session.read_bytes(shared).is_ok());
    session.set_reference(a, 0, None).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(shared), Err(MemoryError::InvalidHandle));
    session.unroot(a).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.stats(region).unwrap().committed_bytes, 0);

    let x = session.alloc_object(region, 1, 1).unwrap();
    let y = session.alloc_object(region, 1, 1).unwrap();
    session.set_reference(x, 0, Some(y)).unwrap();
    session.set_reference(y, 0, Some(x)).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(x), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(y), Err(MemoryError::InvalidHandle));
}

#[test]
fn bad_updates_and_shape_failures_leave_graph_and_charge_unchanged() {
    let mut session = Session::new();
    let a_region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let b_region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let owner = session.alloc_object(a_region, 0, 1).unwrap();
    let old = session.alloc_object(a_region, 0, 0).unwrap();
    let foreign = session.alloc_object(b_region, 0, 0).unwrap();
    let bytes = session.alloc_bytes(a_region, 0).unwrap();
    let stale = session.alloc_object(a_region, 0, 0).unwrap();
    session.set_reference(owner, 0, Some(old)).unwrap();
    session.root(owner).unwrap();
    session.root(bytes).unwrap();
    session.collect(a_region).unwrap();
    assert_eq!(session.read_bytes(stale), Err(MemoryError::InvalidHandle));
    let before = session.stats(a_region).unwrap();
    assert_eq!(
        session.set_reference(owner, 1, None),
        Err(MemoryError::InvalidReferenceSlot)
    );
    assert_eq!(
        session.set_reference(bytes, 0, None),
        Err(MemoryError::NotObject)
    );
    assert_eq!(
        session.set_reference(owner, 0, Some(foreign)),
        Err(MemoryError::CrossRegionEdge)
    );
    assert_eq!(
        session.set_reference(owner, 0, Some(stale)),
        Err(MemoryError::InvalidHandle)
    );
    assert_eq!(session.reference(owner, 0), Ok(Some(old)));
    assert_eq!(session.stats(a_region).unwrap(), before);

    session.collect(b_region).unwrap();
    assert_eq!(
        session.set_reference(owner, 0, Some(foreign)),
        Err(MemoryError::InvalidHandle)
    );
    assert_eq!(session.reference(owner, 0), Ok(Some(old)));
    assert_eq!(
        session.alloc_object(a_region, MAX_OBJECT_PAYLOAD_BYTES + 1, 0),
        Err(AllocationError::ShapeLimit)
    );
    assert_eq!(
        session.alloc_object(a_region, 0, MAX_OBJECT_REFERENCE_SLOTS + 1),
        Err(AllocationError::ShapeLimit)
    );
    let after = session.stats(a_region).unwrap();
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.allocation_failures, before.allocation_failures + 2);
}

#[test]
fn legacy_links_and_checked_slots_are_both_strong() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let bytes = session.alloc_bytes(region, 1).unwrap();
    let object = session.alloc_object(region, 1, 1).unwrap();
    let leaf = session.alloc_object(region, 1, 0).unwrap();
    session.link(bytes, object).unwrap();
    session.set_reference(object, 0, Some(leaf)).unwrap();
    session.root(bytes).unwrap();
    session.collect(region).unwrap();
    assert!(session.read_bytes(leaf).is_ok());
    session.unroot(bytes).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(leaf), Err(MemoryError::InvalidHandle));
}

#[test]
fn nested_guard_fails_atomically_and_never_collects_any_region() {
    let mut session = Session::new();
    let charge = SLOT_METADATA_BYTES + 1 + REFERENCE_SLOT_METADATA_BYTES;
    let auto = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: charge,
            },
            4 * charge,
        )
        .unwrap();
    let manual = session
        .create_region(CollectionMode::Manual, 4 * charge)
        .unwrap();
    let survivor = session.alloc_object(auto, 1, 1).unwrap();
    session.root(survivor).unwrap();
    let before_auto = session.stats(auto).unwrap();
    let before_manual = session.stats(manual).unwrap();
    {
        let mut outer = session.no_gc(2 * charge);
        let first = outer.alloc_object(manual, 1, 1).unwrap();
        {
            let mut inner = outer.no_gc(0);
            assert_eq!(
                inner.alloc_object(auto, 1, 1),
                Err(AllocationError::BudgetExceeded)
            );
            assert_eq!(inner.collect(manual), Err(MemoryError::CollectionForbidden));
            inner.set_reference(survivor, 0, Some(survivor)).unwrap();
            assert_eq!(
                inner.stats(auto).unwrap().collection_count,
                before_auto.collection_count
            );
            assert_eq!(
                inner.stats(manual).unwrap().collection_count,
                before_manual.collection_count
            );
        }
        let second = outer.alloc_object(manual, 1, 1).unwrap();
        assert_eq!(
            outer.alloc_object(manual, 1, 1),
            Err(AllocationError::BudgetExceeded)
        );
        assert!(outer.read_bytes(first).is_ok());
        assert!(outer.read_bytes(second).is_ok());
        assert_eq!(
            outer.stats(auto).unwrap().collection_count,
            before_auto.collection_count
        );
        assert_eq!(
            outer.stats(manual).unwrap().collection_count,
            before_manual.collection_count
        );
    }
    assert_eq!(session.reference(survivor, 0), Ok(Some(survivor)));
    assert_eq!(
        session.stats(manual).unwrap().collection_count,
        before_manual.collection_count
    );
    session.alloc_object(auto, 1, 1).unwrap();
    assert_eq!(
        session.stats(auto).unwrap().collection_count,
        before_auto.collection_count + 1
    );
    assert!(session.read_bytes(survivor).is_ok());
}

#[test]
fn closing_one_region_does_not_touch_another_and_invalidates_handles() {
    let mut session = Session::new();
    let a = session.create_region(CollectionMode::Manual, 1024).unwrap();
    let b = session.create_region(CollectionMode::Manual, 1024).unwrap();
    let left = session.alloc_object(a, 1, 1).unwrap();
    let right = session.alloc_object(b, 1, 1).unwrap();
    session.root(left).unwrap();
    session.root(right).unwrap();
    assert_eq!(session.close_region(a), Err(MemoryError::RegionHasRoots));
    session.unroot(left).unwrap();
    session.close_region(a).unwrap();
    assert_eq!(session.read_bytes(left), Err(MemoryError::InvalidHandle));
    assert!(session.read_bytes(right).is_ok());
    session.collect(b).unwrap();
    assert!(session.read_bytes(right).is_ok());
    assert_eq!(session.memory_report().regions[1].committed_bytes, 0);
}
