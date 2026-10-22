use cgx2_runtime::{AllocationError, CollectionMode, MemoryError, Session, SLOT_METADATA_BYTES};

#[test]
fn default_region_uses_auto_limit_and_trigger() {
    let mut session = Session::new();
    let region = session.default_region();
    assert_eq!(
        session.mode(region),
        Ok(CollectionMode::Auto {
            trigger_bytes: 1 << 20
        })
    );
    assert_eq!(session.limit_bytes(region), Ok(64 << 20));
    let handle = session.alloc_bytes(region, 3).unwrap();
    assert_eq!(session.read_bytes(handle), Ok(&[0, 0, 0][..]));
}

#[test]
fn named_regions_preserve_mode_and_limit() {
    let mut session = Session::new();
    let manual = session.create_region(CollectionMode::Manual, 128).unwrap();
    let auto = session
        .create_region(CollectionMode::Auto { trigger_bytes: 80 }, 256)
        .unwrap();
    assert_eq!(session.mode(manual), Ok(CollectionMode::Manual));
    assert_eq!(session.limit_bytes(manual), Ok(128));
    assert_eq!(
        session.mode(auto),
        Ok(CollectionMode::Auto { trigger_bytes: 80 })
    );
    assert_eq!(session.limit_bytes(auto), Ok(256));
}

#[test]
fn allocation_charges_payload_and_slot_metadata() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 1024).unwrap();
    session.alloc_bytes(region, 7).unwrap();
    let stats = session.stats(region).unwrap();
    assert_eq!(stats.committed_bytes, 7 + SLOT_METADATA_BYTES);
    assert_eq!(stats.live_bytes, 7 + SLOT_METADATA_BYTES);
    assert_eq!(stats.peak_bytes, 7 + SLOT_METADATA_BYTES);
}

#[test]
fn limit_failure_does_not_commit_or_damage_previous_allocation() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, SLOT_METADATA_BYTES + 4)
        .unwrap();
    let first = session.alloc_bytes(region, 4).unwrap();
    let before = session.stats(region).unwrap();
    assert_eq!(
        session.alloc_bytes(region, 1),
        Err(AllocationError::RegionLimit)
    );
    let after = session.stats(region).unwrap();
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.live_bytes, before.live_bytes);
    assert_eq!(after.allocation_failures, before.allocation_failures + 1);
    assert_eq!(session.read_bytes(first), Ok(&[0, 0, 0, 0][..]));
}

#[test]
fn reclaimed_handle_cannot_alias_reused_slot() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 1024).unwrap();
    let stale = session.alloc_bytes(region, 2).unwrap();
    session.collect(region).unwrap();
    let fresh = session.alloc_bytes(region, 2).unwrap();
    assert_eq!(session.read_bytes(stale), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(fresh), Ok(&[0, 0][..]));
    assert_ne!(stale, fresh);
}

#[test]
fn rooted_cycle_survives_and_unrooted_cycle_is_reclaimed() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let a = session.alloc_bytes(region, 2).unwrap();
    let b = session.alloc_bytes(region, 3).unwrap();
    session.link(a, b).unwrap();
    session.link(b, a).unwrap();
    session.root(a).unwrap();
    session.collect(region).unwrap();
    assert!(session.read_bytes(a).is_ok());
    assert!(session.read_bytes(b).is_ok());
    session.unroot(a).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(a), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(b), Err(MemoryError::InvalidHandle));
    assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
}

#[test]
fn manual_region_never_collects_during_allocation() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, 4 * (SLOT_METADATA_BYTES + 1))
        .unwrap();
    for _ in 0..4 {
        session.alloc_bytes(region, 1).unwrap();
    }
    assert_eq!(session.stats(region).unwrap().collection_count, 0);
    assert_eq!(
        session.alloc_bytes(region, 1),
        Err(AllocationError::RegionLimit)
    );
    assert_eq!(session.stats(region).unwrap().collection_count, 0);
    session.collect(region).unwrap();
    assert_eq!(session.stats(region).unwrap().collection_count, 1);
    assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
}

#[test]
fn auto_collects_at_next_allocation_safepoint_after_trigger() {
    let mut session = Session::new();
    let charge = SLOT_METADATA_BYTES + 1;
    let region = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: 2 * charge,
            },
            3 * charge,
        )
        .unwrap();
    let a = session.alloc_bytes(region, 1).unwrap();
    session.alloc_bytes(region, 1).unwrap();
    assert_eq!(session.stats(region).unwrap().collection_count, 0);
    let fresh = session.alloc_bytes(region, 1).unwrap();
    assert_eq!(session.stats(region).unwrap().collection_count, 1);
    assert_eq!(session.read_bytes(a), Err(MemoryError::InvalidHandle));
    assert!(session.read_bytes(fresh).is_ok());
}

#[test]
fn cross_region_edges_are_rejected_without_mutation() {
    let mut session = Session::new();
    let a_region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let b_region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let a = session.alloc_bytes(a_region, 1).unwrap();
    let b = session.alloc_bytes(b_region, 1).unwrap();
    let before = session.stats(a_region).unwrap();
    assert_eq!(session.link(a, b), Err(MemoryError::CrossRegionEdge));
    assert_eq!(session.stats(a_region).unwrap(), before);
    session.root(a).unwrap();
    session.collect(a_region).unwrap();
    session.collect(b_region).unwrap();
    assert!(session.read_bytes(a).is_ok());
    assert_eq!(session.read_bytes(b), Err(MemoryError::InvalidHandle));
}

#[test]
fn rooted_region_cannot_close_and_other_region_survives_close() {
    let mut session = Session::new();
    let a_region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let b_region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let a = session.alloc_bytes(a_region, 1).unwrap();
    let b = session.alloc_bytes(b_region, 1).unwrap();
    session.root(a).unwrap();
    assert_eq!(
        session.close_region(a_region),
        Err(MemoryError::RegionHasRoots)
    );
    assert!(session.read_bytes(a).is_ok());
    session.unroot(a).unwrap();
    session.close_region(a_region).unwrap();
    assert_eq!(session.read_bytes(a), Err(MemoryError::InvalidHandle));
    assert!(session.read_bytes(b).is_ok());
    assert_eq!(session.stats(b_region).unwrap().collection_count, 0);
}

#[test]
fn one_guard_suppresses_two_auto_regions_until_their_next_safepoints() {
    let mut session = Session::new();
    let a = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: SLOT_METADATA_BYTES + 1,
            },
            4096,
        )
        .unwrap();
    let b = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: SLOT_METADATA_BYTES + 1,
            },
            4096,
        )
        .unwrap();
    {
        let mut guard = session.no_gc(4 * (SLOT_METADATA_BYTES + 1));
        guard.alloc_bytes(a, 1).unwrap();
        guard.alloc_bytes(b, 1).unwrap();
        guard.alloc_bytes(a, 1).unwrap();
        guard.alloc_bytes(b, 1).unwrap();
        assert_eq!(guard.stats(a).unwrap().collection_count, 0);
        assert_eq!(guard.stats(b).unwrap().collection_count, 0);
    }
    assert_eq!(session.stats(a).unwrap().collection_count, 0);
    assert_eq!(session.stats(b).unwrap().collection_count, 0);
    session.alloc_bytes(a, 1).unwrap();
    assert_eq!(session.stats(a).unwrap().collection_count, 1);
    assert_eq!(session.stats(b).unwrap().collection_count, 0);
    session.alloc_bytes(b, 1).unwrap();
    assert_eq!(session.stats(b).unwrap().collection_count, 1);
}

#[test]
fn nested_budgets_charge_each_guard_and_first_exhausted_fails() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let charge = SLOT_METADATA_BYTES + 1;
    let mut outer = session.no_gc(3 * charge);
    outer.alloc_bytes(region, 1).unwrap();
    {
        let mut inner = outer.no_gc(charge);
        inner.alloc_bytes(region, 1).unwrap();
        assert_eq!(
            inner.alloc_bytes(region, 1),
            Err(AllocationError::BudgetExceeded)
        );
        assert_eq!(inner.stats(region).unwrap().collection_count, 0);
    }
    outer.alloc_bytes(region, 1).unwrap();
    assert_eq!(
        outer.alloc_bytes(region, 1),
        Err(AllocationError::BudgetExceeded)
    );
    assert_eq!(outer.stats(region).unwrap().committed_bytes, 3 * charge);
}

#[test]
fn budget_failure_is_before_commit_and_preserves_existing_object() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let charge = SLOT_METADATA_BYTES + 4;
    let mut guard = session.no_gc(charge);
    let first = guard.alloc_bytes(region, 4).unwrap();
    let before = guard.stats(region).unwrap();
    assert_eq!(
        guard.alloc_bytes(region, 1),
        Err(AllocationError::BudgetExceeded)
    );
    let after = guard.stats(region).unwrap();
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.allocation_failures, before.allocation_failures + 1);
    assert_eq!(guard.read_bytes(first), Ok(&[0, 0, 0, 0][..]));
    assert_eq!(after.collection_count, 0);
}

#[test]
fn explicit_collection_is_forbidden_in_guard_even_for_other_region() {
    let mut session = Session::new();
    let a = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let b = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let mut guard = session.no_gc(100);
    assert_eq!(guard.collect(a), Err(MemoryError::CollectionForbidden));
    assert_eq!(guard.collect(b), Err(MemoryError::CollectionForbidden));
    assert_eq!(guard.stats(a).unwrap().collection_count, 0);
    assert_eq!(guard.stats(b).unwrap().collection_count, 0);
}

#[test]
fn guard_drop_after_error_restores_collection() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    {
        let mut guard = session.no_gc(0);
        assert_eq!(
            guard.alloc_bytes(region, 1),
            Err(AllocationError::BudgetExceeded)
        );
        assert_eq!(guard.stats(region).unwrap().collection_count, 0);
    }
    session.collect(region).unwrap();
    assert_eq!(session.stats(region).unwrap().collection_count, 1);
}

#[test]
fn duplicate_link_needs_no_budget_or_region_headroom() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let a = session.alloc_bytes(region, 1).unwrap();
    let b = session.alloc_bytes(region, 1).unwrap();
    session.link(a, b).unwrap();
    let before = session.stats(region).unwrap();
    let mut guard = session.no_gc(0);
    guard.link(a, b).unwrap();
    assert_eq!(guard.stats(region).unwrap(), before);
}

#[test]
fn creating_region_inside_guard_charges_global_budget() {
    use cgx2_runtime::REGION_METADATA_BYTES;

    let mut session = Session::new();
    let mut guard = session.no_gc(REGION_METADATA_BYTES);
    guard.create_region(CollectionMode::Manual, 1024).unwrap();
    assert_eq!(
        guard.create_region(CollectionMode::Manual, 1024),
        Err(AllocationError::BudgetExceeded)
    );
}

#[test]
fn deterministic_graph_operations_preserve_reachability_and_handle_validity() {
    use cgx2_runtime::Handle;

    struct Entry {
        handle: Handle,
        rooted: bool,
        alive: bool,
        edges: Vec<usize>,
    }
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, 128 * 1024)
        .unwrap();
    let mut entries: Vec<Entry> = Vec::new();
    let mut seed = 0x5eed_u64;
    for _ in 0..300 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let live: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.alive.then_some(i))
            .collect();
        match seed % 5 {
            0 | 1 if entries.len() < 100 => {
                let handle = session.alloc_bytes(region, (seed % 4) as usize).unwrap();
                entries.push(Entry {
                    handle,
                    rooted: false,
                    alive: true,
                    edges: Vec::new(),
                });
            }
            2 if !live.is_empty() => {
                let i = live[(seed as usize / 7) % live.len()];
                if entries[i].rooted {
                    session.unroot(entries[i].handle).unwrap();
                } else {
                    session.root(entries[i].handle).unwrap();
                }
                entries[i].rooted = !entries[i].rooted;
            }
            3 if live.len() > 1 => {
                let parent = live[(seed as usize / 11) % live.len()];
                let child = live[(seed as usize / 17) % live.len()];
                session
                    .link(entries[parent].handle, entries[child].handle)
                    .unwrap();
                entries[parent].edges.push(child);
            }
            _ => {
                let before = session.stats(region).unwrap().collection_count;
                session.collect(region).unwrap();
                assert_eq!(session.stats(region).unwrap().collection_count, before + 1);
                let mut reachable = vec![false; entries.len()];
                let mut stack: Vec<usize> = entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| (e.alive && e.rooted).then_some(i))
                    .collect();
                while let Some(i) = stack.pop() {
                    if reachable[i] {
                        continue;
                    }
                    reachable[i] = true;
                    stack.extend(
                        entries[i]
                            .edges
                            .iter()
                            .copied()
                            .filter(|&j| entries[j].alive),
                    );
                }
                for (i, entry) in entries.iter_mut().enumerate() {
                    entry.alive &= reachable[i];
                    assert_eq!(session.read_bytes(entry.handle).is_ok(), entry.alive);
                }
            }
        }
        let stats = session.stats(region).unwrap();
        assert_eq!(stats.live_bytes, stats.committed_bytes);
        assert!(stats.peak_bytes >= stats.committed_bytes);
    }
}

#[test]
fn impossible_allocation_counts_as_failure_without_collection() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, usize::MAX)
        .unwrap();
    assert_eq!(
        session.alloc_bytes(region, usize::MAX),
        Err(AllocationError::HostAllocation)
    );
    let stats = session.stats(region).unwrap();
    assert_eq!(stats.allocation_failures, 1);
    assert_eq!(stats.committed_bytes, 0);
    assert_eq!(stats.collection_count, 0);
}
