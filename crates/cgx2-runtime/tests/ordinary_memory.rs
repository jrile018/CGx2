use cgx2_runtime::{AllocationError, CollectionMode, Session};

#[test]
fn graph_copy_preserves_aliases_and_cycles_without_cross_region_handles() {
    let mut session = Session::try_new().unwrap();
    let source_region = session.default_region();
    let destination = session
        .create_region(CollectionMode::Manual, 1 << 20)
        .unwrap();
    let mut roots = session.scoped_roots();
    let source = roots.alloc_object(source_region, 8, 3).unwrap();
    roots.root(source).unwrap();
    roots.write_bytes(source, 0, &42i64.to_le_bytes()).unwrap();
    let child = roots.alloc_object(source_region, 4, 1).unwrap();
    roots.root(child).unwrap();
    roots.set_reference(source, 0, Some(child)).unwrap();
    roots.set_reference(source, 1, Some(child)).unwrap();
    roots.set_reference(source, 2, Some(source)).unwrap();
    roots.set_reference(child, 0, Some(source)).unwrap();
    let copy = roots.copy_graph_to_v1(destination, source).unwrap();
    assert_ne!(source, copy);
    assert_eq!(roots.read_bytes(copy).unwrap(), 42i64.to_le_bytes());
    let copied_child = roots.reference(copy, 0).unwrap().unwrap();
    assert_eq!(copied_child.region(), destination);
    assert_eq!(roots.reference(copy, 1).unwrap(), Some(copied_child));
    assert_eq!(roots.reference(copy, 2).unwrap(), Some(copy));
    assert_eq!(roots.reference(copied_child, 0).unwrap(), Some(copy));
    roots.write_bytes(copied_child, 0, &[1]).unwrap();
    assert_eq!(roots.read_bytes(child).unwrap(), &[0; 4]);
}

#[test]
fn failed_promotion_preserves_original_owner_for_read_and_retry() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let tiny = session.create_region(CollectionMode::Manual, 0).unwrap();
    let destination = session
        .create_region(CollectionMode::Manual, 1 << 20)
        .unwrap();
    let mut roots = session.scoped_roots();
    let source = roots.alloc_object(region, 8, 0).unwrap();
    roots.root(source).unwrap();
    roots.write_bytes(source, 0, &42i64.to_le_bytes()).unwrap();
    let owner = roots.copy_owned_graph_v1(source).unwrap();
    let identity = owner.identity_v1();
    let contents = owner.encoded_bytes().to_vec();
    let baseline = roots.root_census().unwrap();
    let failure = roots.promote_graph_v1(tiny, owner).unwrap_err();
    assert_eq!(failure.error, AllocationError::RegionLimit);
    assert_eq!(failure.owner.identity_v1(), identity);
    assert_eq!(failure.owner.encoded_bytes(), contents);
    assert_eq!(roots.root_census().unwrap(), baseline);
    assert_eq!(roots.stats(tiny).unwrap().committed_bytes, 0);
    let copied = roots.promote_graph_v1(destination, failure.owner).unwrap();
    assert_eq!(roots.read_bytes(copied).unwrap(), 42i64.to_le_bytes());
}

#[cfg(feature = "fault-injection")]
#[test]
fn every_transfer_reserve_failure_removes_prepared_nodes_and_roots() {
    let mut failures = 0;
    for reserve in 1..64 {
        let mut session = Session::try_new().unwrap();
        let source_region = session.default_region();
        let destination = session
            .create_region(CollectionMode::Manual, 1 << 20)
            .unwrap();
        let mut roots = session.scoped_roots();
        let source = roots.alloc_object(source_region, 8, 1).unwrap();
        roots.root(source).unwrap();
        let child = roots.alloc_bytes(source_region, 8).unwrap();
        roots.root(child).unwrap();
        roots.set_reference(source, 0, Some(child)).unwrap();
        let owner = roots.copy_owned_graph_v1(source).unwrap();
        let identity = owner.identity_v1();
        let baseline = roots.root_census().unwrap();
        roots.inject_host_failure_at_reserve(reserve);
        match roots.promote_graph_v1(destination, owner) {
            Ok(_) => {
                assert!(
                    failures >= 5,
                    "all metadata/payload/publication sites must be reached"
                );
                break;
            }
            Err(failure) => {
                failures += 1;
                assert_eq!(failure.error, AllocationError::HostAllocation);
                assert_eq!(failure.owner.identity_v1(), identity);
                assert_eq!(roots.root_census().unwrap(), baseline);
                assert_eq!(roots.stats(destination).unwrap().committed_bytes, 0);
                assert_eq!(roots.stats(destination).unwrap().collection_count, 0);
                roots.disable_host_failure();
                assert!(roots.promote_graph_v1(destination, failure.owner).is_ok());
            }
        }
    }
    assert!(
        failures < 63,
        "fault enumeration must terminate at the first success"
    );
}

#[test]
fn transfer_budgets_are_cumulative_exact_and_never_collect() {
    fn attempt(budget: usize) -> (bool, u128, u64, usize) {
        let mut session = Session::try_new().unwrap();
        let source_region = session.default_region();
        let destination = session
            .create_region(CollectionMode::Auto { trigger_bytes: 1 }, 1 << 20)
            .unwrap();
        let mut roots = session.scoped_roots();
        let source = roots.alloc_object(source_region, 8, 1).unwrap();
        roots.root(source).unwrap();
        roots.set_reference(source, 0, Some(source)).unwrap();
        let before = roots.memory_observation_v2();
        let result = roots
            .with_no_gc(budget, |guard| guard.copy_graph_to_v1(destination, source))
            .unwrap();
        let after = roots.memory_observation_v2();
        let committed = roots.stats(destination).unwrap().committed_bytes;
        (
            result.is_ok(),
            after.charged_bytes - before.charged_bytes,
            after.collection_count - before.collection_count,
            committed,
        )
    }
    let (success, total, collections, _) = attempt(usize::MAX);
    assert!(success);
    assert_eq!(collections, 0);
    // Guard table establishment is charged to outer guards. The newly entered
    // guard starts with zero spent, so remove its own table reserve here.
    let own_guard_table = std::mem::size_of::<usize>() * 2;
    let exact = usize::try_from(total).unwrap() - own_guard_table;
    let (success, _, collections, _) = attempt(exact);
    assert!(success);
    assert_eq!(collections, 0);
    let (success, charged, collections, committed) = attempt(exact - 1);
    assert!(!success);
    assert!(charged > own_guard_table as u128);
    assert_eq!(collections, 0);
    assert_eq!(committed, 0);
    assert!(!attempt(0).0);
}
