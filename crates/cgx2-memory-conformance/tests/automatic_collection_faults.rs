use cgx2_runtime::{
    AllocationError, CollectionMode, MemoryError, ReserveSite, Session,
    REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES,
};

const OBJECT_CHARGE: usize = SLOT_METADATA_BYTES + 1 + REFERENCE_SLOT_METADATA_BYTES;
const BYTE_CHARGE: usize = SLOT_METADATA_BYTES + 1;

#[test]
fn nested_no_gc_defers_each_auto_region_without_collecting_manual_region() {
    let mut session = Session::new();
    let first = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: OBJECT_CHARGE,
            },
            8 * OBJECT_CHARGE,
        )
        .unwrap();
    let second = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: OBJECT_CHARGE,
            },
            8 * OBJECT_CHARGE,
        )
        .unwrap();
    let manual = session
        .create_region(CollectionMode::Manual, 8 * OBJECT_CHARGE)
        .unwrap();
    let stale_first = session.alloc_object(first, 1, 1).unwrap();
    let stale_second = session.alloc_object(second, 1, 1).unwrap();
    let manual_handle = session.alloc_object(manual, 1, 1).unwrap();
    let before = session.memory_report();

    let mut outer = session.no_gc(3 * OBJECT_CHARGE);
    let mut inner = outer.no_gc(OBJECT_CHARGE);
    let retained = inner.alloc_object(first, 1, 1).unwrap();
    inner.root(retained).unwrap();
    assert_eq!(
        inner.alloc_object(second, 1, 1),
        Err(AllocationError::BudgetExceeded)
    );
    for region in [first, second, manual] {
        assert_eq!(inner.collect(region), Err(MemoryError::CollectionForbidden));
    }
    assert_eq!(
        inner.memory_report().totals.collection_count,
        before.totals.collection_count
    );
    drop(inner);
    let deferred_second = outer.alloc_object(second, 1, 1).unwrap();
    assert_eq!(
        outer.memory_report().totals.collection_count,
        before.totals.collection_count
    );
    drop(outer);

    session.alloc_object(first, 1, 1).unwrap();
    let first_report = session.memory_report();
    assert_eq!(
        first_report.totals.collection_count,
        before.totals.collection_count + 1
    );
    assert_eq!(
        session.read_bytes(stale_first),
        Err(MemoryError::InvalidHandle)
    );
    assert_eq!(session.read_bytes(retained), Ok(&[0][..]));
    session.alloc_object(second, 1, 1).unwrap();
    let after = session.memory_report();
    assert_eq!(
        after.totals.collection_count,
        before.totals.collection_count + 2
    );
    assert_eq!(
        session.read_bytes(stale_second),
        Err(MemoryError::InvalidHandle)
    );
    assert_eq!(
        session.read_bytes(deferred_second),
        Err(MemoryError::InvalidHandle)
    );
    assert_eq!(session.read_bytes(manual_handle), Ok(&[0][..]));
    assert_eq!(session.stats(manual).unwrap().collection_count, 0);
    assert_eq!(
        after.totals.allocation_failures,
        before.totals.allocation_failures + 1
    );
}

#[test]
fn object_reserve_faults_precede_auto_safepoint_but_byte_fault_follows_it() {
    let mut session = Session::new();
    let region = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: 2 * OBJECT_CHARGE,
            },
            4 * OBJECT_CHARGE,
        )
        .unwrap();
    let parent = session.alloc_object(region, 1, 1).unwrap();
    let child = session.alloc_object(region, 1, 1).unwrap();
    session.root(parent).unwrap();
    session.set_reference(parent, 0, Some(child)).unwrap();

    for (index, site) in [
        (1, ReserveSite::ObjectPayload),
        (2, ReserveSite::ObjectReferences),
    ] {
        let before = session.stats(region).unwrap();
        session.inject_host_failure_at_reserve(index);
        assert_eq!(
            session.alloc_object(region, 1, 1),
            Err(AllocationError::HostAllocation)
        );
        assert_eq!(session.last_host_failure_site(), Some(site));
        let after = session.stats(region).unwrap();
        assert_eq!(after.collection_count, before.collection_count);
        assert_eq!(after.committed_bytes, before.committed_bytes);
        assert_eq!(after.allocation_failures, before.allocation_failures + 1);
        assert_eq!(session.reference(parent, 0), Ok(Some(child)));
    }

    let before = session.stats(region).unwrap();
    session.inject_host_failure_at_reserve(1);
    assert_eq!(
        session.alloc_bytes(region, 1),
        Err(AllocationError::HostAllocation)
    );
    assert_eq!(
        session.last_host_failure_site(),
        Some(ReserveSite::ManagedBytes)
    );
    let after = session.stats(region).unwrap();
    assert_eq!(after.collection_count, before.collection_count + 1);
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.allocation_failures, before.allocation_failures + 1);
    assert_eq!(session.reference(parent, 0), Ok(Some(child)));
    let next = session.alloc_bytes(region, 1).unwrap();
    assert_eq!(session.read_bytes(next), Ok(&[0][..]));
    session.unroot(parent).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
    assert_eq!(session.read_bytes(parent), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(child), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(next), Err(MemoryError::InvalidHandle));
}

#[test]
fn failed_edge_growth_is_atomic_across_the_next_auto_collection() {
    let mut session = Session::new();
    let region = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: 2 * BYTE_CHARGE,
            },
            4 * BYTE_CHARGE,
        )
        .unwrap();
    let parent = session.alloc_bytes(region, 1).unwrap();
    let child = session.alloc_bytes(region, 1).unwrap();
    session.root(parent).unwrap();
    let before = session.stats(region).unwrap();
    session.inject_host_failure_at_reserve(1);
    assert_eq!(
        session.link(parent, child),
        Err(MemoryError::Allocation(AllocationError::HostAllocation))
    );
    assert_eq!(session.last_host_failure_site(), Some(ReserveSite::Edges));
    let failed = session.stats(region).unwrap();
    assert_eq!(failed.collection_count, before.collection_count);
    assert_eq!(failed.committed_bytes, before.committed_bytes);
    assert_eq!(failed.allocation_failures, before.allocation_failures + 1);

    let replacement = session.alloc_bytes(region, 1).unwrap();
    assert_eq!(session.read_bytes(child), Err(MemoryError::InvalidHandle));
    session.link(parent, replacement).unwrap();
    session.alloc_bytes(region, 1).unwrap();
    assert_eq!(session.read_bytes(replacement), Ok(&[0][..]));
    assert_eq!(
        session.stats(region).unwrap().collection_count,
        before.collection_count + 2
    );
    session.unroot(parent).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
    assert_eq!(session.read_bytes(parent), Err(MemoryError::InvalidHandle));
    assert_eq!(
        session.read_bytes(replacement),
        Err(MemoryError::InvalidHandle)
    );
}

#[test]
fn promotion_preserves_frozen_no_safepoint_contract_and_owned_value_on_failure() {
    let mut session = Session::new();
    let region = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: 2 * BYTE_CHARGE,
            },
            2 * BYTE_CHARGE,
        )
        .unwrap();
    let stale_a = session.alloc_bytes(region, 1).unwrap();
    let stale_b = session.alloc_bytes(region, 1).unwrap();
    let mut value = session.alloc_owned_bytes(1).unwrap();
    value.as_mut_slice()[0] = 7;
    let before = session.stats(region).unwrap();
    let (error, value) = session.promote_owned(region, value).unwrap_err();
    assert_eq!(error, AllocationError::RegionLimit);
    assert_eq!(value.as_slice(), &[7]);
    let after = session.stats(region).unwrap();
    assert_eq!(after.collection_count, before.collection_count);
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.allocation_failures, before.allocation_failures + 1);
    session.collect(region).unwrap();
    let promoted = session.promote_owned(region, value).unwrap();
    let after = session.stats(region).unwrap();
    assert_eq!(after.collection_count, before.collection_count + 1);
    assert_eq!(after.total_collected_bytes, 2 * BYTE_CHARGE);
    assert_eq!(after.committed_bytes, BYTE_CHARGE);
    assert_eq!(session.read_bytes(stale_a), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(stale_b), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(promoted), Ok(&[7][..]));

    // An impossible manual request is classified before an armed host reserve.
    let manual = session
        .create_region(CollectionMode::Manual, BYTE_CHARGE)
        .unwrap();
    let rooted = session.alloc_bytes(manual, 1).unwrap();
    session.root(rooted).unwrap();
    let value = session.alloc_owned_bytes(1).unwrap();
    session.inject_host_failure_at_reserve(1);
    let (error, value) = session.promote_owned(manual, value).unwrap_err();
    assert_eq!(error, AllocationError::RegionLimit);
    assert_eq!(session.host_reserve_attempts(), 0);
    assert_eq!(session.last_host_failure_site(), None);
    assert_eq!(value.as_slice(), &[0]);
    session.disable_host_failure();
    session.unroot(rooted).unwrap();
    session.collect(manual).unwrap();
    let promoted_manual = session.promote_owned(manual, value).unwrap();
    assert_eq!(session.read_bytes(promoted_manual), Ok(&[0][..]));
}
