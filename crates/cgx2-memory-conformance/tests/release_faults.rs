use cgx2_runtime::{
    AllocationError, CollectionMode, MemoryError, RegionId, ReserveSite, Session,
    EDGE_METADATA_BYTES, SLOT_METADATA_BYTES,
};

fn manual(session: &mut Session, limit: usize) -> RegionId {
    session
        .create_region(CollectionMode::Manual, limit)
        .unwrap()
}

#[test]
fn object_slot_reservation_failure_rolls_back_and_releases_once() {
    let mut session = Session::new();
    let region = manual(&mut session, 1 << 20);
    let parent = session.alloc_object(region, 1, 1).unwrap();
    let child = session.alloc_object(region, 1, 0).unwrap();
    session.root(parent).unwrap();
    session.root(child).unwrap();
    session.set_reference(parent, 0, Some(child)).unwrap();

    // Make the next object need a new slot table reservation after its payload
    // and reference vectors have already been reserved.
    loop {
        let (len, capacity) = session.host_slot_table_capacity(region).unwrap();
        if len == capacity {
            break;
        }
        session.alloc_object(region, 1, 1).unwrap();
    }
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
    assert_eq!(session.host_reserve_attempts(), 3);
    let failed = session.stats(region).unwrap();
    assert_eq!(failed.committed_bytes, before.committed_bytes);
    assert_eq!(failed.live_bytes, before.live_bytes);
    assert_eq!(failed.collection_count, before.collection_count);
    assert_eq!(failed.allocation_failures, before.allocation_failures + 1);
    assert_eq!(session.reference(parent, 0), Ok(Some(child)));

    let replacement = session.alloc_object(region, 1, 1).unwrap();
    session.root(replacement).unwrap();
    assert_eq!(
        session.close_region(region),
        Err(MemoryError::RegionHasRoots)
    );
    session.unroot(parent).unwrap();
    session.unroot(child).unwrap();
    session.unroot(replacement).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(parent), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(child), Err(MemoryError::InvalidHandle));
    assert_eq!(
        session.read_bytes(replacement),
        Err(MemoryError::InvalidHandle)
    );
    assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
    session.close_region(region).unwrap();
    assert_eq!(
        session.close_region(region),
        Err(MemoryError::InvalidRegion)
    );
}

#[test]
fn legacy_edge_reservation_is_atomic_and_known_manual_limit_precedes_fault() {
    let mut session = Session::new();
    let exact = 2 * (SLOT_METADATA_BYTES + 1);
    let limited = manual(&mut session, exact);
    let parent = session.alloc_object(limited, 1, 0).unwrap();
    let child = session.alloc_object(limited, 1, 0).unwrap();
    session.root(parent).unwrap();
    session.inject_host_failure_at_reserve(1);
    assert_eq!(
        session.link(parent, child),
        Err(MemoryError::Allocation(AllocationError::RegionLimit))
    );
    assert_eq!(session.host_reserve_attempts(), 0);
    assert_eq!(session.last_host_failure_site(), None);
    assert_eq!(session.stats(limited).unwrap().committed_bytes, exact);
    session.unroot(parent).unwrap();
    session.collect(limited).unwrap();
    assert_eq!(session.stats(limited).unwrap().committed_bytes, 0);
    session.disable_host_failure();

    let region = manual(&mut session, 4096);
    let parent = session.alloc_object(region, 1, 0).unwrap();
    let child = session.alloc_object(region, 1, 0).unwrap();
    session.root(parent).unwrap();
    let before = session.stats(region).unwrap();
    session.inject_host_failure_at_reserve(1);
    assert_eq!(
        session.link(parent, child),
        Err(MemoryError::Allocation(AllocationError::HostAllocation))
    );
    assert_eq!(session.last_host_failure_site(), Some(ReserveSite::Edges));
    assert_eq!(
        session.stats(region).unwrap().committed_bytes,
        before.committed_bytes
    );
    assert_eq!(
        session.stats(region).unwrap().allocation_failures,
        before.allocation_failures + 1
    );
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(child), Err(MemoryError::InvalidHandle));
    let new_child = session.alloc_object(region, 1, 0).unwrap();
    assert_ne!(child, new_child);
    session.link(parent, new_child).unwrap();
    assert!(
        session.stats(region).unwrap().committed_bytes
            >= before.committed_bytes + EDGE_METADATA_BYTES
    );
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(new_child), Ok(&[0][..]));
    session.unroot(parent).unwrap();
    session.collect(region).unwrap();
    assert_eq!(
        session.read_bytes(new_child),
        Err(MemoryError::InvalidHandle)
    );
    assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
}

#[test]
fn fixed_point_collection_needs_no_host_worklist_reservation() {
    let mut session = Session::new();
    let region = manual(&mut session, 4096);
    let parent = session.alloc_object(region, 1, 1).unwrap();
    let child = session.alloc_object(region, 1, 0).unwrap();
    session.set_reference(parent, 0, Some(child)).unwrap();
    session.root(parent).unwrap();
    session.inject_host_failure_at_reserve(1);
    session.set_reference(parent, 0, None).unwrap();
    session.set_reference(parent, 0, Some(child)).unwrap();
    assert_eq!(session.host_reserve_attempts(), 0);
    let before = session.stats(region).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.host_reserve_attempts(), 0);
    assert_eq!(session.last_host_failure_site(), None);
    assert_eq!(
        session.stats(region).unwrap().collection_count,
        before.collection_count + 1
    );
    assert_eq!(session.read_bytes(child), Ok(&[0][..]));

    // The armed fault still belongs to the next actual host reservation.
    assert_eq!(
        session.alloc_object(region, 1, 0),
        Err(AllocationError::HostAllocation)
    );
    assert_eq!(
        session.last_host_failure_site(),
        Some(ReserveSite::ObjectPayload)
    );
    assert_eq!(
        session.stats(region).unwrap().allocation_failures,
        before.allocation_failures + 1
    );
    let next = session.alloc_object(region, 1, 0).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(next), Err(MemoryError::InvalidHandle));
    session.unroot(parent).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
}
