use cgx2_runtime::{AllocationError, CollectionMode, MemoryError, Session, SLOT_METADATA_BYTES};

fn auto(limit_bytes: usize, trigger_bytes: usize) -> (Session, cgx2_runtime::RegionId) {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Auto { trigger_bytes }, limit_bytes)
        .unwrap();
    (session, region)
}

#[test]
fn invalid_and_closed_regions_cannot_collect_or_change_other_regions() {
    let (mut session, region) = auto(2 * SLOT_METADATA_BYTES, SLOT_METADATA_BYTES);
    let retained = session.alloc_bytes(region, 0).unwrap();
    session.root(retained).unwrap();
    let foreign = Session::new().default_region();
    let before = session.memory_report();
    assert_eq!(
        session.alloc_bytes(foreign, 0),
        Err(AllocationError::InvalidRegion)
    );
    assert_eq!(session.memory_report().totals, before.totals);
    assert_eq!(session.read_bytes(retained), Ok(&[][..]));

    session.unroot(retained).unwrap();
    session.close_region(region).unwrap();
    let closed = session.memory_report();
    assert_eq!(
        session.alloc_bytes(region, 0),
        Err(AllocationError::InvalidRegion)
    );
    assert_eq!(session.memory_report().totals, closed.totals);
    assert_eq!(
        session.read_bytes(retained),
        Err(MemoryError::InvalidHandle)
    );
}

#[test]
fn overflow_and_intrinsically_oversized_requests_fail_before_auto_collection() {
    let (mut session, region) = auto(2 * SLOT_METADATA_BYTES, SLOT_METADATA_BYTES);
    let retained = session.alloc_bytes(region, 0).unwrap();
    session.root(retained).unwrap();
    let before = session.memory_report();

    assert_eq!(
        session.alloc_bytes(region, usize::MAX),
        Err(AllocationError::HostAllocation)
    );
    assert_eq!(
        session.alloc_bytes(region, SLOT_METADATA_BYTES + 1),
        Err(AllocationError::RegionLimit)
    );
    let after = session.memory_report();
    assert_eq!(
        after.regions[1].committed_bytes,
        before.regions[1].committed_bytes
    );
    assert_eq!(after.regions[1].live_bytes, before.regions[1].live_bytes);
    assert_eq!(after.regions[1].peak_bytes, before.regions[1].peak_bytes);
    assert_eq!(
        after.regions[1].allocation_failures,
        before.regions[1].allocation_failures + 2
    );
    assert_eq!(
        after.regions[1].collection_count,
        before.regions[1].collection_count
    );
    assert_eq!(
        after.regions[1].pause.sample_count,
        before.regions[1].pause.sample_count
    );
    assert_eq!(session.read_bytes(retained), Ok(&[][..]));
    assert_eq!(after.regions[0], before.regions[0]);
}

#[test]
fn host_capacity_overflow_fails_before_auto_collection() {
    let (mut session, region) = auto(usize::MAX, 0);
    let retained = session.alloc_bytes(region, 0).unwrap();
    session.root(retained).unwrap();
    let before = session.memory_report();
    assert_eq!(
        session.alloc_bytes(region, isize::MAX as usize + 1),
        Err(AllocationError::HostAllocation)
    );
    let after = session.memory_report();
    assert_eq!(
        after.regions[1].collection_count,
        before.regions[1].collection_count
    );
    assert_eq!(
        after.regions[1].committed_bytes,
        before.regions[1].committed_bytes
    );
    assert_eq!(
        after.regions[1].allocation_failures,
        before.regions[1].allocation_failures + 1
    );
    assert_eq!(session.read_bytes(retained), Ok(&[][..]));
}

#[test]
fn auto_reclaims_on_capacity_pressure_below_trigger() {
    let charge = SLOT_METADATA_BYTES + 1;
    let (mut session, region) = auto(charge, 3 * charge);
    let stale = session.alloc_bytes(region, 1).unwrap();
    let before = session.memory_report();
    let fresh = session.alloc_bytes(region, 1).unwrap();
    let after = session.memory_report();
    assert_eq!(
        after.regions[1].collection_count,
        before.regions[1].collection_count + 1
    );
    assert_eq!(after.regions[1].total_collected_bytes, charge);
    assert_eq!(after.regions[1].committed_bytes, charge);
    assert_eq!(after.regions[1].allocation_failures, 0);
    assert_eq!(session.read_bytes(stale), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(fresh), Ok(&[0][..]));
    assert_ne!(stale, fresh);
}

#[test]
fn auto_collection_cannot_make_rooted_capacity_available() {
    let charge = SLOT_METADATA_BYTES + 1;
    let (mut session, region) = auto(charge, 3 * charge);
    let retained = session.alloc_bytes(region, 1).unwrap();
    session.root(retained).unwrap();
    let before = session.memory_report();
    assert_eq!(
        session.alloc_bytes(region, 1),
        Err(AllocationError::RegionLimit)
    );
    let after = session.memory_report();
    assert_eq!(
        after.regions[1].collection_count,
        before.regions[1].collection_count + 1
    );
    assert_eq!(
        after.regions[1].allocation_failures,
        before.regions[1].allocation_failures + 1
    );
    assert_eq!(
        after.regions[1].committed_bytes,
        before.regions[1].committed_bytes
    );
    assert_eq!(after.regions[1].live_bytes, before.regions[1].live_bytes);
    assert_eq!(after.regions[1].peak_bytes, before.regions[1].peak_bytes);
    assert_eq!(session.read_bytes(retained), Ok(&[0][..]));
    assert_eq!(after.regions[0], before.regions[0]);
}

#[test]
fn manual_failure_waits_for_explicit_collection() {
    let mut session = Session::new();
    let charge = SLOT_METADATA_BYTES + 1;
    let region = session
        .create_region(CollectionMode::Manual, charge)
        .unwrap();
    let stale = session.alloc_bytes(region, 1).unwrap();
    let before = session.memory_report();
    assert_eq!(
        session.alloc_bytes(region, 1),
        Err(AllocationError::RegionLimit)
    );
    let failed = session.memory_report();
    assert_eq!(failed.regions[1].collection_count, 0);
    assert_eq!(
        failed.regions[1].committed_bytes,
        before.regions[1].committed_bytes
    );
    assert_eq!(failed.regions[1].allocation_failures, 1);
    assert_eq!(session.read_bytes(stale), Ok(&[0][..]));
    session.collect(region).unwrap();
    let fresh = session.alloc_bytes(region, 1).unwrap();
    assert_eq!(session.read_bytes(stale), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(fresh), Ok(&[0][..]));
    assert_eq!(session.stats(region).unwrap().collection_count, 1);
}

#[test]
fn nested_guards_exhaust_budget_without_collecting_any_region() {
    let charge = SLOT_METADATA_BYTES + 1;
    let (mut session, auto_region) = auto(charge, 0);
    let manual = session
        .create_region(CollectionMode::Manual, charge)
        .unwrap();
    let survivor = session.alloc_bytes(auto_region, 1).unwrap();
    session.root(survivor).unwrap();
    let before = session.memory_report();
    {
        let mut outer = session.no_gc(2 * charge);
        {
            let mut inner = outer.no_gc(charge - 1);
            assert_eq!(
                inner.alloc_bytes(auto_region, 1),
                Err(AllocationError::BudgetExceeded)
            );
            assert_eq!(
                inner.alloc_bytes(manual, 1),
                Err(AllocationError::BudgetExceeded)
            );
            assert_eq!(
                inner.collect(auto_region),
                Err(MemoryError::CollectionForbidden)
            );
            assert_eq!(
                inner.memory_report().regions[1].collection_count,
                before.regions[1].collection_count
            );
        }
        let owned = outer.alloc_bytes(manual, 1).unwrap();
        assert_eq!(
            outer.alloc_bytes(auto_region, 1),
            Err(AllocationError::RegionLimit)
        );
        assert_eq!(
            outer.alloc_bytes(manual, 1),
            Err(AllocationError::RegionLimit)
        );
        assert!(outer.alloc_owned_bytes(0).is_ok());
        assert_eq!(outer.read_bytes(owned), Ok(&[0][..]));
        assert_eq!(outer.read_bytes(survivor), Ok(&[0][..]));
        assert_eq!(
            outer.memory_report().totals.collection_count,
            before.totals.collection_count
        );
    }
    let after = session.memory_report();
    assert_eq!(
        after.totals.collection_count,
        before.totals.collection_count
    );
    assert_eq!(after.regions[1].allocation_failures, 2);
    assert_eq!(after.regions[2].allocation_failures, 2);
    assert_eq!(after.regions[1].committed_bytes, charge);
    assert_eq!(after.regions[2].committed_bytes, charge);
}
