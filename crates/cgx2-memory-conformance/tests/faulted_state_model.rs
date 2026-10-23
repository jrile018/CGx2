//! Bounded direct-runtime oracle: two regions, at most 14 live nodes, two checked
//! references per object, and the fixed seed/step fixture. The oracle
//! stores logical node IDs and charges, never runtime slot or mark state.
use cgx2_runtime::{
    AllocationError, CollectionMode, Handle, MemoryError, RegionId, RegionStats, RootCensus,
    Session, REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES,
};

const MAX_LIVE: usize = 14;
const BYTE_CHARGE: usize = SLOT_METADATA_BYTES + 1;
const OBJECT_CHARGE: usize = BYTE_CHARGE + 2 * REFERENCE_SLOT_METADATA_BYTES;

#[derive(Clone, Copy)]
struct Node {
    handle: Handle,
    region: usize,
    charge: usize,
    object: bool,
    roots: usize,
    refs: [Option<usize>; 2],
    live: bool,
}

struct Model {
    nodes: Vec<Node>,
    stats: [RegionStats; 2],
    closed: [bool; 2],
}

impl Model {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            stats: [RegionStats::default(); 2],
            closed: [false; 2],
        }
    }

    fn live(&self, region: usize) -> Vec<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(id, n)| (n.live && n.region == region).then_some(id))
            .collect()
    }

    fn add(&mut self, handle: Handle, region: usize, object: bool) -> usize {
        let charge = if object { OBJECT_CHARGE } else { BYTE_CHARGE };
        let stats = &mut self.stats[region];
        stats.committed_bytes += charge;
        stats.live_bytes += charge;
        stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
        let id = self.nodes.len();
        self.nodes.push(Node {
            handle,
            region,
            charge,
            object,
            roots: 0,
            refs: [None; 2],
            live: true,
        });
        id
    }

    fn collect(&mut self, region: usize) {
        let mut seen = vec![false; self.nodes.len()];
        let mut pending: Vec<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, n)| (n.live && n.region == region && n.roots > 0).then_some(id))
            .collect();
        while let Some(id) = pending.pop() {
            if seen[id] || !self.nodes[id].live {
                continue;
            }
            seen[id] = true;
            pending.extend(self.nodes[id].refs.into_iter().flatten());
        }
        for (id, node) in self.nodes.iter_mut().enumerate() {
            if node.live && node.region == region && !seen[id] {
                node.live = false;
                self.stats[region].committed_bytes -= node.charge;
                self.stats[region].live_bytes -= node.charge;
                self.stats[region].total_collected_bytes += node.charge;
            }
        }
        self.stats[region].collection_count += 1;
    }

    fn close(&mut self, region: usize) {
        for node in &mut self.nodes {
            if node.region == region {
                node.live = false;
            }
        }
        self.stats[region].committed_bytes = 0;
        self.stats[region].live_bytes = 0;
        self.closed[region] = true;
    }

    fn census(&self, region: Option<usize>) -> RootCensus {
        let mut census = RootCensus::default();
        for node in &self.nodes {
            if node.live && region.is_none_or(|r| r == node.region) && node.roots > 0 {
                census.rooted_handles += 1;
                census.root_registrations += node.roots;
            }
        }
        census
    }
}

fn check(session: &Session, model: &Model, regions: [RegionId; 2], trace: &str, step: usize) {
    assert!(model.nodes.iter().filter(|n| n.live).count() <= MAX_LIVE);
    assert_eq!(
        session.root_census(),
        Ok(model.census(None)),
        "{trace} step {step}"
    );
    let report = session.memory_report();
    let mut totals = RegionStats::default();
    for (r, region) in regions.into_iter().enumerate() {
        let expected = if model.closed[r] {
            Err(MemoryError::InvalidRegion)
        } else {
            Ok(model.stats[r])
        };
        assert_eq!(
            session.stats(region),
            expected,
            "{trace} step {step} region {r}"
        );
        assert_eq!(
            session.root_census_region(region),
            if model.closed[r] {
                Err(MemoryError::InvalidRegion)
            } else {
                Ok(model.census(Some(r)))
            },
            "{trace} step {step} region {r} census"
        );
        let row = report.regions.iter().find(|row| row.id == region).unwrap();
        assert_eq!(
            row.is_open, !model.closed[r],
            "{trace} step {step} region {r}"
        );
        assert_eq!(row.committed_bytes, model.stats[r].committed_bytes);
        assert_eq!(row.live_bytes, model.stats[r].live_bytes);
        assert_eq!(row.peak_bytes, model.stats[r].peak_bytes);
        assert_eq!(row.allocation_failures, model.stats[r].allocation_failures);
        assert_eq!(row.collection_count, model.stats[r].collection_count);
        assert_eq!(
            row.total_collected_bytes,
            model.stats[r].total_collected_bytes
        );
        totals.committed_bytes += model.stats[r].committed_bytes;
        totals.live_bytes += model.stats[r].live_bytes;
        totals.peak_bytes += model.stats[r].peak_bytes;
        totals.allocation_failures += model.stats[r].allocation_failures;
        totals.collection_count += model.stats[r].collection_count;
        totals.total_collected_bytes += model.stats[r].total_collected_bytes;
    }
    assert_eq!(report.totals, totals, "{trace} step {step} report totals");
    for (id, node) in model.nodes.iter().enumerate() {
        if node.live {
            assert_eq!(
                session.read_bytes(node.handle),
                Ok(&[0][..]),
                "{trace} step {step} node {id}"
            );
            if node.object {
                for slot in 0..2 {
                    assert_eq!(
                        session.reference(node.handle, slot),
                        Ok(node.refs[slot].map(|child| model.nodes[child].handle)),
                        "{trace} step {step} node {id} reference {slot}"
                    );
                }
            }
        } else {
            assert_eq!(
                session.read_bytes(node.handle),
                Err(MemoryError::InvalidHandle),
                "{trace} step {step} stale node {id}"
            );
        }
    }
}

fn next(state: &mut u64, choices: usize) -> usize {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*state >> 32) as usize % choices
}

fn fixtures() -> Vec<(u64, usize)> {
    include_str!("../corpus/wave18-state-seeds.csv")
        .lines()
        .skip(1)
        .map(|line| {
            let (seed, steps) = line.split_once(',').unwrap();
            (
                u64::from_str_radix(seed.trim_start_matches("0x"), 16).unwrap(),
                steps.parse().unwrap(),
            )
        })
        .collect()
}

fn allocate(
    session: &mut Session,
    model: &mut Model,
    region: RegionId,
    r: usize,
    kind: usize,
    guarded: bool,
) {
    // Auto collection is due before a successful ordinary allocation at trigger 0.
    // Promotion does not run the collector, and no_gc suppresses it.
    if r == 1 && kind != 2 && !guarded {
        model.collect(r);
    }
    let handle = match kind {
        0 => session.alloc_bytes(region, 1).unwrap(),
        1 => session.alloc_object(region, 1, 2).unwrap(),
        _ => {
            let owned = session.alloc_owned_bytes(1).unwrap();
            session.promote_owned(region, owned).unwrap()
        }
    };
    model.add(handle, r, kind == 1);
}

#[test]
fn bounded_root_graph_and_region_state_replay() {
    for (seed, steps) in fixtures() {
        let trace = format!("seed {seed:#x}");
        let mut state = seed;
        let mut session = Session::new();
        let regions = [
            session.create_region(CollectionMode::Manual, 4096).unwrap(),
            session
                .create_region(CollectionMode::Auto { trigger_bytes: 0 }, 4096)
                .unwrap(),
        ];
        let mut model = Model::new();
        check(&session, &model, regions, &trace, 0);
        for step in 0..steps {
            let r = next(&mut state, 2);
            let live = model.live(r);
            match next(&mut state, 8) {
                0..=2 if model.nodes.iter().filter(|n| n.live).count() < MAX_LIVE - 2 => {
                    allocate(
                        &mut session,
                        &mut model,
                        regions[r],
                        r,
                        next(&mut state, 3),
                        false,
                    );
                }
                3 if !live.is_empty() => {
                    let id = live[next(&mut state, live.len())];
                    session.root(model.nodes[id].handle).unwrap();
                    model.nodes[id].roots += 1;
                }
                4 => {
                    let rooted: Vec<_> = live
                        .into_iter()
                        .filter(|&id| model.nodes[id].roots > 0)
                        .collect();
                    if !rooted.is_empty() {
                        let id = rooted[next(&mut state, rooted.len())];
                        session.unroot(model.nodes[id].handle).unwrap();
                        model.nodes[id].roots -= 1;
                    }
                }
                5 => {
                    let owners: Vec<_> = live
                        .iter()
                        .copied()
                        .filter(|&id| model.nodes[id].object)
                        .collect();
                    if !owners.is_empty() {
                        let owner = owners[next(&mut state, owners.len())];
                        let slot = next(&mut state, 2);
                        let target = (!live.is_empty() && next(&mut state, 3) != 0)
                            .then(|| live[next(&mut state, live.len())]);
                        session
                            .set_reference(
                                model.nodes[owner].handle,
                                slot,
                                target.map(|id| model.nodes[id].handle),
                            )
                            .unwrap();
                        model.nodes[owner].refs[slot] = target;
                    }
                }
                6 => {
                    session.collect(regions[r]).unwrap();
                    model.collect(r);
                }
                _ => {}
            }
            check(&session, &model, regions, &trace, step + 1);
            if step == 15 || step == 39 {
                let mut outer = session.no_gc(4096);
                let mut inner = outer.no_gc(4096);
                allocate(&mut inner, &mut model, regions[r], r, 1, true);
                check(&inner, &model, regions, &trace, step + 100);
                assert_eq!(
                    inner.collect(regions[r]),
                    Err(MemoryError::CollectionForbidden)
                );
                check(&inner, &model, regions, &trace, step + 101);
                drop(inner);
                assert_eq!(
                    outer.collect(regions[r]),
                    Err(MemoryError::CollectionForbidden)
                );
                drop(outer);
                check(&session, &model, regions, &trace, step + 102);
            }
        }
        for r in 0..2 {
            for id in model.live(r) {
                while model.nodes[id].roots > 0 {
                    session.unroot(model.nodes[id].handle).unwrap();
                    model.nodes[id].roots -= 1;
                    check(&session, &model, regions, &trace, steps + id);
                }
            }
            session.collect(regions[r]).unwrap();
            model.collect(r);
            check(&session, &model, regions, &trace, steps + r);
            session.close_region(regions[r]).unwrap();
            model.close(r);
            check(&session, &model, regions, &trace, steps + r + 2);
        }
    }
}

#[test]
fn duplicate_roots_protect_cycle_until_final_release() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let a = session.alloc_object(region, 1, 2).unwrap();
    let b = session.alloc_object(region, 1, 2).unwrap();
    session.set_reference(a, 0, Some(b)).unwrap();
    session.set_reference(b, 0, Some(a)).unwrap();
    session.root(a).unwrap();
    session.root(a).unwrap();
    assert_eq!(
        session.root_census_region(region),
        Ok(RootCensus {
            rooted_handles: 1,
            root_registrations: 2
        })
    );
    assert_eq!(session.root_census(), session.root_census_region(region));
    assert_eq!(
        session.close_region(region),
        Err(MemoryError::RegionHasRoots)
    );
    assert_eq!(
        session.root_census_region(region),
        Ok(RootCensus {
            rooted_handles: 1,
            root_registrations: 2
        })
    );
    session.unroot(a).unwrap();
    session.collect(region).unwrap();
    assert!(session.read_bytes(b).is_ok());
    assert_eq!(
        session.root_census(),
        Ok(RootCensus {
            rooted_handles: 1,
            root_registrations: 1
        })
    );
    let mut guard = session.no_gc(0);
    guard.unroot(a).unwrap();
    assert_eq!(guard.collect(region), Err(MemoryError::CollectionForbidden));
    assert!(guard.read_bytes(b).is_ok());
    drop(guard);
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(a), Err(MemoryError::InvalidHandle));
    assert_eq!(session.read_bytes(b), Err(MemoryError::InvalidHandle));
    assert_eq!(
        session.root_census_region(region),
        Ok(RootCensus::default())
    );
    assert_eq!(session.root_census(), session.root_census_region(region));
}

#[test]
fn owned_buffer_promotion_fault_preserves_value_and_model() {
    use cgx2_runtime::ReserveSite;
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let regions = [region, session.default_region()];
    let mut model = Model::new();
    let owned = session.alloc_owned_bytes(1).unwrap();
    check(&session, &model, regions, "owned allocation", 0);
    session.inject_host_failure_at_reserve(1);
    let (error, owned) = session.promote_owned(region, owned).unwrap_err();
    assert_eq!(error, AllocationError::HostAllocation);
    assert_eq!(owned.as_slice(), &[0]);
    assert_eq!(
        session.last_host_failure_site(),
        Some(ReserveSite::SlotTable)
    );
    assert_eq!(session.host_reserve_attempts(), 1);
    model.stats[0].allocation_failures += 1;
    check(&session, &model, regions, "failed promotion", 1);
    session.disable_host_failure();
    let handle = session.promote_owned(region, owned).unwrap();
    model.add(handle, 0, false);
    check(&session, &model, regions, "promotion recovery", 2);
    session.root(handle).unwrap();
    model.nodes[0].roots = 1;
    check(&session, &model, regions, "promoted root", 3);
    let copy = session.copy_managed_to_owned(handle).unwrap();
    assert_eq!(copy.as_slice(), &[0]);
    check(&session, &model, regions, "owned copy", 4);
    drop(copy);
    check(&session, &model, regions, "owned drop", 5);
    session.unroot(handle).unwrap();
    model.nodes[0].roots = 0;
    check(&session, &model, regions, "promoted unroot", 6);
    session.collect(region).unwrap();
    model.collect(0);
    check(&session, &model, regions, "promoted collect", 7);
}

#[test]
fn replay_one_based_host_reserve_failures() {
    use cgx2_runtime::ReserveSite;
    for (point, site) in [
        (1, ReserveSite::ObjectPayload),
        (2, ReserveSite::ObjectReferences),
        (3, ReserveSite::SlotTable),
    ] {
        let mut session = Session::new();
        let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
        let regions = [region, session.default_region()];
        let mut model = Model::new();
        session.inject_host_failure_at_reserve(point);
        assert_eq!(
            session.alloc_object(region, 1, 2),
            Err(AllocationError::HostAllocation),
            "object reserve {point}"
        );
        model.stats[0].allocation_failures += 1;
        check(&session, &model, regions, "object reserve", point);
        assert_eq!(session.host_reserve_attempts(), point);
        assert_eq!(session.last_host_failure_site(), Some(site));
        assert_eq!(session.root_census(), Ok(RootCensus::default()));
        assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
        assert_eq!(session.stats(region).unwrap().allocation_failures, 1);
        session.disable_host_failure();
        let handle = session.alloc_object(region, 1, 2).unwrap();
        model.add(handle, 0, true);
        check(&session, &model, regions, "object recovery", point);
        assert_eq!(session.reference(handle, 0), Ok(None));
        assert_eq!(session.root_census(), Ok(RootCensus::default()));
    }

    for (point, site) in [(1, ReserveSite::ManagedBytes), (2, ReserveSite::SlotTable)] {
        let mut session = Session::new();
        let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
        let regions = [region, session.default_region()];
        let mut model = Model::new();
        session.inject_host_failure_at_reserve(point);
        assert_eq!(
            session.alloc_bytes(region, 1),
            Err(AllocationError::HostAllocation),
            "bytes reserve {point}"
        );
        model.stats[0].allocation_failures += 1;
        check(&session, &model, regions, "bytes reserve", point);
        assert_eq!(session.last_host_failure_site(), Some(site));
        assert_eq!(session.host_reserve_attempts(), point);
        session.disable_host_failure();
        let handle = session.alloc_bytes(region, 1).unwrap();
        model.add(handle, 0, false);
        check(&session, &model, regions, "bytes recovery", point);
    }

    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
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
    assert_eq!(session.host_reserve_attempts(), 1);
    let after = session.stats(region).unwrap();
    assert_eq!(after.committed_bytes, before.committed_bytes);
    assert_eq!(after.live_bytes, before.live_bytes);
    assert_eq!(after.allocation_failures, before.allocation_failures + 1);
    assert_eq!(
        session.root_census(),
        Ok(RootCensus {
            rooted_handles: 1,
            root_registrations: 1
        })
    );
    session.collect(region).unwrap();
    assert_eq!(
        session.read_bytes(child),
        Err(MemoryError::InvalidHandle),
        "failed edge must not retain child"
    );
    assert!(session.read_bytes(parent).is_ok());

    for site in [ReserveSite::OwnedBytes, ReserveSite::ManagedToOwned] {
        let mut session = Session::new();
        let region = session.default_region();
        let handle = session.alloc_bytes(region, 1).unwrap();
        session.root(handle).unwrap();
        let before = session.stats(region).unwrap();
        session.inject_host_failure_at_reserve(1);
        match site {
            ReserveSite::OwnedBytes => assert!(matches!(
                session.alloc_owned_bytes(1),
                Err(AllocationError::HostAllocation)
            )),
            ReserveSite::ManagedToOwned => assert!(matches!(
                session.copy_managed_to_owned(handle),
                Err(MemoryError::Allocation(AllocationError::HostAllocation))
            )),
            _ => unreachable!(),
        }
        assert_eq!(session.last_host_failure_site(), Some(site));
        assert_eq!(session.host_reserve_attempts(), 1);
        assert_eq!(session.stats(region), Ok(before));
        assert_eq!(
            session.root_census(),
            Ok(RootCensus {
                rooted_handles: 1,
                root_registrations: 1
            })
        );
    }
}
