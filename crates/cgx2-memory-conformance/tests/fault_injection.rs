use cgx2_runtime::{
    AllocationError, CollectionMode, MemoryError, ReserveSite, Session, SLOT_METADATA_BYTES,
};

fn manual() -> (Session, cgx2_runtime::RegionId) {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, 1 << 20)
        .unwrap();
    (session, region)
}

fn graph(
    session: &mut Session,
    region: cgx2_runtime::RegionId,
) -> (cgx2_runtime::Handle, cgx2_runtime::Handle) {
    let owner = session.alloc_object(region, 3, 1).unwrap();
    let target = session.alloc_object(region, 2, 0).unwrap();
    session.root(owner).unwrap();
    session.root(target).unwrap();
    session.set_reference(owner, 0, Some(target)).unwrap();
    (owner, target)
}

fn fill_slot_table(session: &mut Session, region: cgx2_runtime::RegionId, rooted: bool) {
    loop {
        let (len, capacity) = session.host_slot_table_capacity(region).unwrap();
        if len == capacity {
            return;
        }
        let handle = session.alloc_bytes(region, 1).unwrap();
        if rooted {
            session.root(handle).unwrap();
        }
    }
}

fn assert_graph_unchanged(
    session: &mut Session,
    region: cgx2_runtime::RegionId,
    owner: cgx2_runtime::Handle,
    target: cgx2_runtime::Handle,
    before: cgx2_runtime::RegionStats,
) {
    let after = session.stats(region).unwrap();
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.live_bytes, before.live_bytes);
    assert_eq!(after.peak_bytes, before.peak_bytes);
    assert_eq!(after.total_collected_bytes, before.total_collected_bytes);
    assert_eq!(after.collection_count, before.collection_count);
    assert_eq!(after.allocation_failures, before.allocation_failures + 1);
    assert_eq!(session.read_bytes(owner).unwrap(), &[0, 0, 0]);
    assert_eq!(session.read_bytes(target).unwrap(), &[0, 0]);
    assert_eq!(session.reference(owner, 0), Ok(Some(target)));
    assert_eq!(
        session.memory_report().totals.allocation_failures,
        after.allocation_failures
    );
    assert_eq!(
        session.close_region(region),
        Err(MemoryError::RegionHasRoots)
    );
    session.unroot(owner).unwrap();
    session.unroot(target).unwrap();
    assert_eq!(session.unroot(owner), Err(MemoryError::NotRooted));
    assert_eq!(session.unroot(target), Err(MemoryError::NotRooted));
}

#[test]
fn every_managed_reserve_can_fail_without_committing_graph_state() {
    let cases = [
        (false, 1, ReserveSite::ManagedBytes),
        (false, 2, ReserveSite::SlotTable),
        (true, 1, ReserveSite::ObjectPayload),
        (true, 2, ReserveSite::ObjectReferences),
        (true, 3, ReserveSite::SlotTable),
    ];
    for (object, index, site) in cases {
        let (mut session, region) = manual();
        let (owner, target) = graph(&mut session, region);
        if site == ReserveSite::SlotTable {
            fill_slot_table(&mut session, region, false);
        }
        let before = session.stats(region).unwrap();
        session.inject_host_failure_at_reserve(index);
        let result = if object {
            session.alloc_object(region, 4, 2)
        } else {
            session.alloc_bytes(region, 4)
        };
        assert_eq!(result, Err(AllocationError::HostAllocation), "{site:?}");
        assert_eq!(session.host_reserve_attempts(), index, "{site:?}");
        assert_eq!(session.last_host_failure_site(), Some(site));
        assert_graph_unchanged(&mut session, region, owner, target, before);
        // A failed slot reserve did not create a live object or consume its slot.
        let handle = if object {
            session.alloc_object(region, 4, 2).unwrap()
        } else {
            session.alloc_bytes(region, 4).unwrap()
        };
        assert_eq!(session.read_bytes(handle).unwrap(), &[0; 4]);
    }
}

#[test]
fn logical_manual_region_limit_precedes_injected_host_failure() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, SLOT_METADATA_BYTES + 1)
        .unwrap();
    let first = session.alloc_object(region, 1, 0).unwrap();
    session.root(first).unwrap();
    let before = session.stats(region).unwrap();
    session.inject_host_failure_at_reserve(1);
    assert_eq!(
        session.alloc_object(region, 1, 0),
        Err(AllocationError::RegionLimit)
    );
    assert_eq!(session.host_reserve_attempts(), 0);
    assert_eq!(session.last_host_failure_site(), None);
    let after = session.stats(region).unwrap();
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.collection_count, before.collection_count);
    assert_eq!(after.allocation_failures, before.allocation_failures + 1);
    assert_eq!(session.read_bytes(first).unwrap(), &[0]);
}

#[test]
fn conversion_and_edge_reserves_retain_ownership_and_references() {
    let (mut session, region) = manual();
    let (owner, target) = graph(&mut session, region);

    session.inject_host_failure_at_reserve(1);
    assert!(matches!(
        session.alloc_owned_bytes(5),
        Err(AllocationError::HostAllocation)
    ));
    assert_eq!(
        session.last_host_failure_site(),
        Some(ReserveSite::OwnedBytes)
    );

    session.inject_host_failure_at_reserve(1);
    assert!(matches!(
        session.copy_managed_to_owned(owner),
        Err(MemoryError::Allocation(AllocationError::HostAllocation))
    ));
    assert_eq!(
        session.last_host_failure_site(),
        Some(ReserveSite::ManagedToOwned)
    );
    assert_eq!(session.reference(owner, 0), Ok(Some(target)));

    let value = session.alloc_owned_bytes(5).unwrap();
    fill_slot_table(&mut session, region, false);
    session.inject_host_failure_at_reserve(1);
    let (error, value) = session.promote_owned(region, value).unwrap_err();
    assert_eq!(error, AllocationError::HostAllocation);
    assert_eq!(
        session.last_host_failure_site(),
        Some(ReserveSite::SlotTable)
    );
    assert_eq!(value.as_slice(), &[0; 5]);
    let promoted = session.promote_owned(region, value).unwrap();
    assert_eq!(session.read_bytes(promoted).unwrap(), &[0; 5]);

    let before = session.stats(region).unwrap();
    session.inject_host_failure_at_reserve(1);
    assert_eq!(
        session.link(owner, target),
        Err(MemoryError::Allocation(AllocationError::HostAllocation))
    );
    assert_eq!(session.last_host_failure_site(), Some(ReserveSite::Edges));
    assert_eq!(session.reference(owner, 0), Ok(Some(target)));
    assert_eq!(
        session.stats(region).unwrap().committed_bytes,
        before.committed_bytes
    );
    assert_eq!(
        session.stats(region).unwrap().allocation_failures,
        before.allocation_failures + 1
    );
    session.link(owner, target).unwrap();
    session.unroot(owner).unwrap();
    session.unroot(target).unwrap();
}

#[test]
fn region_table_failure_and_nested_no_gc_failure_preserve_counters() {
    let mut session = Session::new();
    let before = session.memory_report();
    session.inject_host_failure_at_reserve(1);
    assert_eq!(
        session.create_region(CollectionMode::Manual, 1024),
        Err(AllocationError::HostAllocation)
    );
    assert_eq!(
        session.last_host_failure_site(),
        Some(ReserveSite::RegionTable)
    );
    assert_eq!(session.memory_report().totals, before.totals);
    assert_eq!(session.memory_report().regions.len(), before.regions.len());

    let region = session
        .create_region(CollectionMode::Auto { trigger_bytes: 0 }, 4096)
        .unwrap();
    let mut outer = session.no_gc(1024);
    let mut inner = outer.no_gc(512);
    let before = inner.stats(region).unwrap();
    inner.inject_host_failure_at_reserve(1);
    assert_eq!(
        inner.alloc_object(region, 1, 1),
        Err(AllocationError::HostAllocation)
    );
    let failed = inner.stats(region).unwrap();
    assert_eq!(failed.collection_count, before.collection_count);
    assert_eq!(failed.committed_bytes, before.committed_bytes);
    assert_eq!(failed.allocation_failures, before.allocation_failures + 1);
    let handle = inner.alloc_object(region, 1, 1).unwrap();
    inner.root(handle).unwrap();
    fill_slot_table(&mut inner, region, true);
    inner.inject_host_failure_at_reserve(3);
    assert_eq!(
        inner.alloc_object(region, 1, 1),
        Err(AllocationError::HostAllocation)
    );
    assert_eq!(inner.last_host_failure_site(), Some(ReserveSite::SlotTable));
    assert_eq!(
        inner.stats(region).unwrap().collection_count,
        before.collection_count
    );
    drop(inner);
    let second = outer.alloc_bytes(region, 1).unwrap();
    assert_eq!(
        outer.stats(region).unwrap().collection_count,
        before.collection_count
    );
    drop(outer);
    // An automatic safepoint is allowed only after both guards have exited.
    let _ = session.alloc_bytes(region, 1).unwrap();
    assert_eq!(
        session.stats(region).unwrap().collection_count,
        before.collection_count + 1
    );
    assert_eq!(session.read_bytes(handle), Ok(&[0][..]));
    assert_eq!(session.read_bytes(second), Err(MemoryError::InvalidHandle));
    assert_eq!(session.memory_report().totals.collection_count, 1);
    assert!(session.stats(region).unwrap().committed_bytes >= SLOT_METADATA_BYTES);
}

#[test]
fn auto_safepoint_before_failed_slot_reserve_is_reported() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Auto { trigger_bytes: 0 }, 4096)
        .unwrap();
    let retained = session.alloc_object(region, 1, 1).unwrap();
    session.root(retained).unwrap();
    fill_slot_table(&mut session, region, true);
    let before = session.stats(region).unwrap();
    session.inject_host_failure_at_reserve(3);
    assert_eq!(
        session.alloc_object(region, 1, 1),
        Err(AllocationError::HostAllocation)
    );
    assert_eq!(
        session.last_host_failure_site(),
        Some(ReserveSite::SlotTable)
    );
    let after = session.stats(region).unwrap();
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.live_bytes, before.live_bytes);
    assert_eq!(after.collection_count, before.collection_count + 1);
    assert_eq!(after.allocation_failures, before.allocation_failures + 1);
    assert_eq!(session.reference(retained, 0), Ok(None));
    session.unroot(retained).unwrap();
    assert_eq!(session.unroot(retained), Err(MemoryError::NotRooted));
}
