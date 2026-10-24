use cgx2_runtime::{CollectionMode, Handle, Session};

fn cycle(
    roots: &mut cgx2_runtime::ScopedRoots<'_>,
    region: cgx2_runtime::RegionId,
) -> (Handle, Handle) {
    let source = roots.alloc_object(region, 8, 3).unwrap();
    roots.root(source).unwrap();
    roots.write_bytes(source, 0, &42i64.to_le_bytes()).unwrap();
    let child = roots.alloc_object(region, 4, 1).unwrap();
    roots.root(child).unwrap();
    roots.write_bytes(child, 0, &[1, 2, 3, 4]).unwrap();
    roots.set_reference(source, 0, Some(child)).unwrap();
    roots.set_reference(source, 1, Some(child)).unwrap();
    roots.set_reference(source, 2, Some(source)).unwrap();
    roots.set_reference(child, 0, Some(source)).unwrap();
    roots.release(child).unwrap();
    roots.release(source).unwrap();
    (source, child)
}

fn intact(roots: &cgx2_runtime::ScopedRoots<'_>, source: Handle, child: Handle) {
    assert_eq!(roots.read_bytes(source).unwrap(), 42i64.to_le_bytes());
    assert_eq!(roots.read_bytes(child).unwrap(), [1, 2, 3, 4]);
    assert_eq!(roots.reference(source, 0).unwrap(), Some(child));
    assert_eq!(roots.reference(source, 1).unwrap(), Some(child));
    assert_eq!(roots.reference(source, 2).unwrap(), Some(source));
    assert_eq!(roots.reference(child, 0).unwrap(), Some(source));
}

#[test]
fn unrooted_input_survives_manual_auto_same_and_different_region_copy() {
    for mode in [
        CollectionMode::Manual,
        CollectionMode::Auto { trigger_bytes: 1 },
    ] {
        for same in [false, true] {
            let mut session = Session::try_new().unwrap();
            let region = session.create_region(mode, 1 << 20).unwrap();
            let destination = if same {
                region
            } else {
                session.create_region(mode, 1 << 20).unwrap()
            };
            let (source, child) = cycle(&mut session.scoped_roots(), region);
            let mut roots = session.scoped_roots();
            let before = roots.root_census().unwrap().root_registrations;
            let copy = roots.copy_graph_to_v1(destination, source).unwrap();
            intact(&roots, source, child);
            let copied_child = roots.reference(copy, 0).unwrap().unwrap();
            intact(&roots, copy, copied_child);
            assert_ne!(copy, source);
            assert_ne!(copied_child, child);
            assert_eq!(copy.region(), destination);
            assert_eq!(copied_child.region(), destination);
            assert_eq!(roots.root_census().unwrap().root_registrations, before + 1);
            roots.write_bytes(copied_child, 0, &[9]).unwrap();
            intact(&roots, source, child);
            if matches!(mode, CollectionMode::Auto { .. }) {
                assert!(roots.stats(destination).unwrap().collection_count > 0);
            } else {
                assert_eq!(roots.stats(destination).unwrap().collection_count, 0);
            }
            let cleanup = roots.cleanup();
            assert_eq!(cleanup.released_roots, 1);
            assert_eq!(cleanup.first_error, None);
            assert_eq!(roots.root_census().unwrap().root_registrations, before);
        }
    }
}

#[test]
fn failed_destination_and_guard_budget_preserve_unrooted_source_and_census() {
    let mut session = Session::try_new().unwrap();
    let region = session
        .create_region(CollectionMode::Manual, 1 << 20)
        .unwrap();
    let tiny = session.create_region(CollectionMode::Manual, 0).unwrap();
    let mut roots = session.scoped_roots();
    let (source, child) = cycle(&mut roots, region);
    let baseline = roots.root_census().unwrap();
    assert!(roots.copy_graph_to_v1(tiny, source).is_err());
    assert_eq!(roots.stats(tiny).unwrap().committed_bytes, 0);
    intact(&roots, source, child);
    assert_eq!(roots.root_census().unwrap(), baseline);
    let result = roots
        .with_no_gc(0, |guard| guard.copy_graph_to_v1(region, source))
        .unwrap();
    assert!(result.is_err());
    intact(&roots, source, child);
    assert_eq!(roots.root_census().unwrap(), baseline);
    assert_eq!(roots.stats(region).unwrap().collection_count, 0);
    assert_eq!(roots.memory_observation_v2().active_guards, 0);
}

#[test]
fn exact_and_one_under_copy_budgets_preserve_unrooted_input() {
    fn attempt(mode: CollectionMode, same: bool, budget: usize) -> (bool, usize) {
        let mut session = Session::try_new().unwrap();
        let region = session.create_region(mode, 1 << 20).unwrap();
        let destination = if same {
            region
        } else {
            session.create_region(mode, 1 << 20).unwrap()
        };
        let (source, child) = cycle(&mut session.scoped_roots(), region);
        let mut roots = session.scoped_roots();
        let committed = roots.stats(destination).unwrap().committed_bytes;
        let census = roots.root_census().unwrap();
        let (succeeded, spent) = roots
            .with_no_gc(budget, |guard| {
                let before = guard.memory_observation_v2();
                let result = guard.copy_graph_to_v1(destination, source);
                let after = guard.memory_observation_v2();
                assert_eq!(after.collection_count, before.collection_count);
                if let Ok(copy) = result {
                    guard.release(copy).unwrap();
                }
                (
                    result.is_ok(),
                    usize::try_from(after.charged_bytes - before.charged_bytes).unwrap(),
                )
            })
            .unwrap();
        intact(&roots, source, child);
        assert_eq!(roots.root_census().unwrap(), census);
        assert_eq!(roots.memory_observation_v2().active_guards, 0);
        if !succeeded {
            assert_eq!(roots.stats(destination).unwrap().committed_bytes, committed);
        }
        (succeeded, spent)
    }
    for mode in [
        CollectionMode::Manual,
        CollectionMode::Auto { trigger_bytes: 1 },
    ] {
        for same in [false, true] {
            let (success, exact) = attempt(mode, same, usize::MAX);
            assert!(success);
            assert!(exact > 0);
            assert!(attempt(mode, same, exact).0);
            assert!(!attempt(mode, same, exact - 1).0);
            assert!(!attempt(mode, same, 0).0);
        }
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn every_copy_reserve_failure_preserves_source_and_rolls_back_destination() {
    for mode in [
        CollectionMode::Manual,
        CollectionMode::Auto { trigger_bytes: 1 },
    ] {
        for same in [false, true] {
            let mut failures = 0;
            let mut sites = std::collections::HashSet::new();
            for fail_at in 1..128 {
                let mut session = Session::try_new().unwrap();
                let region = session.create_region(mode, 1 << 20).unwrap();
                let destination = if same {
                    region
                } else {
                    session.create_region(mode, 1 << 20).unwrap()
                };
                let (source, child) = cycle(&mut session.scoped_roots(), region);
                // Fresh table: enumerate source protection and publication-slot
                // reserves separately, as well as prepared-graph root reserves.
                let mut roots = session.scoped_roots();
                let before = roots.stats(destination).unwrap().committed_bytes;
                let census = roots.root_census().unwrap();
                roots.inject_host_failure_at_reserve(fail_at);
                match roots.copy_graph_to_v1(destination, source) {
                    Ok(copy) => {
                        intact(&roots, source, child);
                        assert!(
                            failures >= 10,
                            "must traverse publication and all graph reserves"
                        );
                        assert!(sites.contains("ScopedRoots"));
                        roots.release(copy).unwrap();
                        assert_eq!(roots.root_census().unwrap(), census);
                        println!("copy-faults mode={mode:?} same={same} failures={failures} sites={sites:?}");
                        break;
                    }
                    Err(error) => {
                        failures += 1;
                        assert_eq!(
                            error,
                            cgx2_runtime::GraphError::Memory(
                                cgx2_runtime::MemoryError::Allocation(
                                    cgx2_runtime::AllocationError::HostAllocation
                                )
                            )
                        );
                        sites.insert(format!("{:?}", roots.last_host_failure_site_v2().unwrap()));
                        intact(&roots, source, child);
                        assert_eq!(roots.stats(destination).unwrap().committed_bytes, before);
                        assert_eq!(roots.root_census().unwrap(), census);
                        roots.disable_host_failure();
                        let retry = roots.copy_graph_to_v1(destination, source).unwrap();
                        intact(&roots, source, child);
                        roots.release(retry).unwrap();
                        assert_eq!(roots.root_census().unwrap(), census);
                    }
                }
                assert!(fail_at < 127, "fault enumeration must reach first success");
            }
        }
    }
}
