use cgx2_runtime::{
    AllocationError, CollectionMode, MemoryError, Session, OWNED_METADATA_BYTES,
    SLOT_METADATA_BYTES,
};

#[test]
fn zero_length_conversion_charges_metadata_and_stales_only_after_collection() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, SLOT_METADATA_BYTES)
        .unwrap();
    let owned = session.alloc_owned_bytes(0).unwrap();
    let handle = session.promote_owned(region, owned).unwrap();
    assert!(session.read_bytes(handle).unwrap().is_empty());
    let copy = session.copy_managed_to_owned(handle).unwrap();
    assert!(copy.is_empty());
    let report = session.memory_report();
    assert_eq!(report.regions[1].committed_bytes, SLOT_METADATA_BYTES);
    assert_eq!(report.regions[1].peak_bytes, SLOT_METADATA_BYTES);
    assert_eq!(report.regions[1].collection_count, 0);
    session.root(handle).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(handle).unwrap(), &[]);
    session.unroot(handle).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(handle), Err(MemoryError::InvalidHandle));
    assert_eq!(
        session.copy_managed_to_owned(handle).unwrap_err(),
        MemoryError::InvalidHandle
    );
    let report = session.memory_report();
    assert_eq!(report.regions[1].committed_bytes, 0);
    assert_eq!(report.regions[1].total_collected_bytes, SLOT_METADATA_BYTES);
    assert_eq!(report.regions[1].collection_count, 2);
}

#[test]
fn failed_near_limit_promotion_is_atomic_and_other_region_stays_untouched() {
    let mut session = Session::new();
    let a = session
        .create_region(CollectionMode::Manual, SLOT_METADATA_BYTES + 1)
        .unwrap();
    let b = session
        .create_region(CollectionMode::Manual, SLOT_METADATA_BYTES + 2)
        .unwrap();
    let mut value = session.alloc_owned_bytes(2).unwrap();
    value.as_mut_slice().copy_from_slice(&[13, 17]);
    let before = session.memory_report();
    let (error, value) = session.promote_owned(a, value).unwrap_err();
    assert_eq!(error, AllocationError::RegionLimit);
    assert_eq!(value.as_slice(), &[13, 17]);
    let failed = session.memory_report();
    assert_eq!(
        failed.regions[1].committed_bytes,
        before.regions[1].committed_bytes
    );
    assert_eq!(failed.regions[1].live_bytes, before.regions[1].live_bytes);
    assert_eq!(failed.regions[1].collection_count, 0);
    assert_eq!(failed.regions[1].allocation_failures, 1);
    assert_eq!(failed.regions[2].allocation_failures, 0);
    let promoted = session.promote_owned(b, value).unwrap();
    assert_eq!(session.read_bytes(promoted).unwrap(), &[13, 17]);
    let result = session.memory_report();
    assert_eq!(result.regions[1].committed_bytes, 0);
    assert_eq!(result.regions[2].committed_bytes, SLOT_METADATA_BYTES + 2);
    assert_eq!(result.regions[2].collection_count, 0);
}

#[test]
fn conversion_handles_obey_cross_region_edges_roots_and_close() {
    let mut session = Session::new();
    let a = session.create_region(CollectionMode::Manual, 1024).unwrap();
    let b = session.create_region(CollectionMode::Manual, 1024).unwrap();
    let owned_x = session.alloc_owned_bytes(1).unwrap();
    let x = session.promote_owned(a, owned_x).unwrap();
    let owned_y = session.alloc_owned_bytes(1).unwrap();
    let y = session.promote_owned(b, owned_y).unwrap();
    let before = session.memory_report();
    assert_eq!(session.link(x, y), Err(MemoryError::CrossRegionEdge));
    assert_eq!(
        session.memory_report().regions[1].committed_bytes,
        before.regions[1].committed_bytes
    );
    session.root(x).unwrap();
    assert_eq!(session.close_region(a), Err(MemoryError::RegionHasRoots));
    session.collect(b).unwrap();
    assert_eq!(session.read_bytes(y), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(x).unwrap(), &[0]);
    session.unroot(x).unwrap();
    session.close_region(a).unwrap();
    assert_eq!(session.read_bytes(x), Err(MemoryError::InvalidHandle));
    let report = session.memory_report();
    assert_eq!(report.regions[1].collection_count, 0);
    assert_eq!(report.regions[1].committed_bytes, 0);
    assert_eq!(report.regions[2].collection_count, 1);
    assert_eq!(
        report.regions[2].total_collected_bytes,
        SLOT_METADATA_BYTES + 1
    );
}

#[test]
fn failed_nested_promotion_and_copy_do_not_collect_or_consume_source() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Auto { trigger_bytes: 1 }, 1024)
        .unwrap();
    let mut value = session.alloc_owned_bytes(1).unwrap();
    value.as_mut_slice()[0] = 27;
    let mut outer = session.no_gc(SLOT_METADATA_BYTES + OWNED_METADATA_BYTES);
    let value = {
        let mut inner = outer.no_gc(SLOT_METADATA_BYTES - 1);
        let (error, original) = inner.promote_owned(region, value).unwrap_err();
        assert_eq!(error, AllocationError::BudgetExceeded);
        assert_eq!(original.as_slice(), &[27]);
        assert_eq!(inner.memory_report().regions[1].collection_count, 0);
        original
    };
    let handle = outer.promote_owned(region, value).unwrap();
    assert_eq!(outer.read_bytes(handle).unwrap(), &[27]);
    assert_eq!(
        outer.copy_managed_to_owned(handle).unwrap_err(),
        MemoryError::Allocation(AllocationError::BudgetExceeded)
    );
    assert_eq!(outer.read_bytes(handle).unwrap(), &[27]);
    let report = outer.memory_report();
    assert_eq!(report.regions[1].collection_count, 0);
    assert_eq!(report.regions[1].allocation_failures, 1);
    assert_eq!(report.regions[1].committed_bytes, SLOT_METADATA_BYTES + 1);
}
