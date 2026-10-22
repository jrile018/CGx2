use cgx2_runtime::{
    AllocationError, CollectionMode, MemoryError, Session, OWNED_METADATA_BYTES,
    SLOT_METADATA_BYTES,
};

#[test]
fn owned_allocation_is_zero_filled_and_can_be_mutated() {
    let mut session = Session::new();
    let mut value = session.alloc_owned_bytes(3).unwrap();
    assert_eq!(value.len(), 3);
    assert!(!value.is_empty());
    assert_eq!(value.as_slice(), &[0, 0, 0]);
    value.as_mut_slice()[1] = 42;
    assert_eq!(value.as_slice(), &[0, 42, 0]);

    let moved = value;
    assert_eq!(moved.as_slice(), &[0, 42, 0]);
}

#[test]
fn zero_length_owned_allocation_is_valid() {
    let mut session = Session::new();
    let value = session.alloc_owned_bytes(0).unwrap();
    assert_eq!(value.len(), 0);
    assert!(value.is_empty());
    assert!(value.as_slice().is_empty());
}

#[test]
fn impossible_owned_allocation_fails_without_panicking() {
    let mut session = Session::new();
    assert!(matches!(
        session.alloc_owned_bytes(usize::MAX),
        Err(AllocationError::HostAllocation)
    ));
}

#[test]
fn owned_allocation_charges_active_guard_without_collecting() {
    let mut session = Session::new();
    let region = session.default_region();
    let mut guard = session.no_gc(OWNED_METADATA_BYTES + 3);
    let value = guard.alloc_owned_bytes(3).unwrap();
    assert_eq!(value.as_slice(), &[0, 0, 0]);
    assert!(matches!(
        guard.alloc_owned_bytes(1),
        Err(AllocationError::BudgetExceeded)
    ));
    assert_eq!(guard.stats(region).unwrap().collection_count, 0);
}

#[test]
fn promotion_moves_bytes_into_region_and_obeys_roots() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, SLOT_METADATA_BYTES + 3)
        .unwrap();
    let mut owned = session.alloc_owned_bytes(3).unwrap();
    owned.as_mut_slice().copy_from_slice(&[7, 8, 9]);
    let handle = session.promote_owned(region, owned).unwrap();
    assert_eq!(session.read_bytes(handle).unwrap(), &[7, 8, 9]);
    assert_eq!(
        session.stats(region).unwrap().committed_bytes,
        SLOT_METADATA_BYTES + 3
    );
    session.root(handle).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(handle).unwrap(), &[7, 8, 9]);
    session.unroot(handle).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(handle), Err(MemoryError::InvalidHandle));
}

#[test]
fn region_limit_failure_returns_unchanged_owned_bytes() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, SLOT_METADATA_BYTES + 1)
        .unwrap();
    let mut owned = session.alloc_owned_bytes(2).unwrap();
    owned.as_mut_slice().copy_from_slice(&[4, 5]);
    let before = session.stats(region).unwrap();
    let (error, owned) = session.promote_owned(region, owned).unwrap_err();
    assert_eq!(error, AllocationError::RegionLimit);
    assert_eq!(owned.as_slice(), &[4, 5]);
    let after = session.stats(region).unwrap();
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.live_bytes, before.live_bytes);
    assert_eq!(after.collection_count, before.collection_count);
    assert_eq!(after.allocation_failures, before.allocation_failures + 1);
}

#[test]
fn invalid_region_failure_returns_unchanged_owned_bytes() {
    let mut owner = Session::new();
    let foreign_region = Session::new().default_region();
    let mut owned = owner.alloc_owned_bytes(1).unwrap();
    owned.as_mut_slice()[0] = 21;
    let (error, owned) = owner.promote_owned(foreign_region, owned).unwrap_err();
    assert_eq!(error, AllocationError::InvalidRegion);
    assert_eq!(owned.as_slice(), &[21]);
}

#[test]
fn nested_budget_failure_does_not_commit_promotion_or_collect() {
    let mut session = Session::new();
    let region = session.default_region();
    let mut owned = session.alloc_owned_bytes(2).unwrap();
    owned.as_mut_slice().copy_from_slice(&[11, 12]);
    let mut outer = session.no_gc(SLOT_METADATA_BYTES);
    let before = outer.stats(region).unwrap();
    let owned = {
        let mut inner = outer.no_gc(SLOT_METADATA_BYTES - 1);
        let (error, owned) = inner.promote_owned(region, owned).unwrap_err();
        assert_eq!(error, AllocationError::BudgetExceeded);
        assert_eq!(owned.as_slice(), &[11, 12]);
        let after = inner.stats(region).unwrap();
        assert_eq!(after.committed_bytes, before.committed_bytes);
        assert_eq!(after.live_bytes, before.live_bytes);
        assert_eq!(after.collection_count, before.collection_count);
        assert_eq!(after.allocation_failures, before.allocation_failures + 1);
        owned
    };
    let handle = outer.promote_owned(region, owned).unwrap();
    assert_eq!(outer.read_bytes(handle).unwrap(), &[11, 12]);
    assert_eq!(outer.stats(region).unwrap().collection_count, 0);
}

#[test]
fn managed_copy_is_independent_and_preserves_source_handle() {
    let mut session = Session::new();
    let region = session.default_region();
    let mut owned = session.alloc_owned_bytes(3).unwrap();
    owned.as_mut_slice().copy_from_slice(&[1, 2, 3]);
    let handle = session.promote_owned(region, owned).unwrap();
    let mut copy = session.copy_managed_to_owned(handle).unwrap();
    copy.as_mut_slice()[0] = 99;
    assert_eq!(copy.as_slice(), &[99, 2, 3]);
    assert_eq!(session.read_bytes(handle).unwrap(), &[1, 2, 3]);
}

#[test]
fn copying_stale_handle_returns_invalid_handle() {
    let mut session = Session::new();
    let region = session.default_region();
    let handle = session.alloc_bytes(region, 1).unwrap();
    session.collect(region).unwrap();
    assert!(matches!(
        session.copy_managed_to_owned(handle),
        Err(MemoryError::InvalidHandle)
    ));
}

#[test]
fn managed_copy_budget_failure_keeps_source_valid() {
    let mut session = Session::new();
    let region = session.default_region();
    let handle = session.alloc_bytes(region, 2).unwrap();
    let before = session.stats(region).unwrap();
    let mut guard = session.no_gc(OWNED_METADATA_BYTES + 1);
    assert!(matches!(
        guard.copy_managed_to_owned(handle),
        Err(MemoryError::Allocation(AllocationError::BudgetExceeded))
    ));
    assert_eq!(guard.read_bytes(handle).unwrap(), &[0, 0]);
    assert_eq!(guard.stats(region).unwrap(), before);
}

#[test]
fn zero_length_managed_copy_has_empty_owned_payload() {
    let mut session = Session::new();
    let region = session.default_region();
    let handle = session.alloc_bytes(region, 0).unwrap();
    let copy = session.copy_managed_to_owned(handle).unwrap();
    assert!(copy.as_slice().is_empty());
    assert_eq!(copy.len(), 0);
    assert_eq!(session.read_bytes(handle).unwrap(), &[]);
}

#[test]
fn nested_guards_charge_owned_allocations_without_collection() {
    let mut session = Session::new();
    let region = session.default_region();
    let charge = OWNED_METADATA_BYTES + 2;
    let mut outer = session.no_gc(2 * charge);
    let first = outer.alloc_owned_bytes(2).unwrap();
    {
        let mut inner = outer.no_gc(charge);
        let second = inner.alloc_owned_bytes(2).unwrap();
        assert!(matches!(
            inner.alloc_owned_bytes(1),
            Err(AllocationError::BudgetExceeded)
        ));
        assert_eq!(second.as_slice(), &[0, 0]);
        assert_eq!(inner.stats(region).unwrap().collection_count, 0);
    }
    assert!(matches!(
        outer.alloc_owned_bytes(1),
        Err(AllocationError::BudgetExceeded)
    ));
    assert_eq!(first.as_slice(), &[0, 0]);
    assert_eq!(outer.stats(region).unwrap().collection_count, 0);
}
