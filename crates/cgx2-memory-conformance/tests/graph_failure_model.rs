//! Independent reachability oracle. The model stores logical node IDs, not runtime slots.
use cgx2_runtime::{
    AllocationError, CollectionMode, Handle, MemoryError, RegionId, RegionStats, Session,
    REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES,
};

const SEEDS: u64 = 64;
const OPERATIONS_PER_SEED: usize = 96;
const RANDOM_OPERATIONS_PER_SEED: usize = OPERATIONS_PER_SEED + OPERATIONS_PER_SEED / 24;
const CHARGE: usize = SLOT_METADATA_BYTES + 1 + 2 * REFERENCE_SLOT_METADATA_BYTES;

struct Node {
    handle: Handle,
    region: usize,
    roots: usize,
    refs: [Option<usize>; 2],
    live: bool,
}

struct Oracle {
    nodes: Vec<Node>,
    stats: [RegionStats; 2],
    closed: [bool; 2],
}

impl Oracle {
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
            .filter_map(|(id, node)| (node.live && node.region == region).then_some(id))
            .collect()
    }

    fn collect(&mut self, region: usize) {
        let mut reachable = vec![false; self.nodes.len()];
        let mut frontier: Vec<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, node)| {
                (node.live && node.region == region && node.roots > 0).then_some(id)
            })
            .collect();
        while let Some(id) = frontier.pop() {
            if reachable[id] {
                continue;
            }
            reachable[id] = true;
            frontier.extend(self.nodes[id].refs.into_iter().flatten());
        }
        for (id, node) in self.nodes.iter_mut().enumerate() {
            if node.live && node.region == region && !reachable[id] {
                node.live = false;
                self.stats[region].committed_bytes -= CHARGE;
                self.stats[region].live_bytes -= CHARGE;
                self.stats[region].total_collected_bytes += CHARGE;
            }
        }
        self.stats[region].collection_count += 1;
    }

    fn before_allocation(
        &mut self,
        region: usize,
        guarded: bool,
        fail: bool,
    ) -> Result<(), AllocationError> {
        if fail {
            // Payload/reference reserves precede the auto safepoint.
            self.stats[region].allocation_failures += 1;
            return Err(AllocationError::HostAllocation);
        }
        if region == 1 && !guarded && self.stats[region].committed_bytes >= 3 * CHARGE {
            self.collect(region);
        }
        self.stats[region].committed_bytes += CHARGE;
        self.stats[region].live_bytes += CHARGE;
        self.stats[region].peak_bytes = self.stats[region]
            .peak_bytes
            .max(self.stats[region].committed_bytes);
        Ok(())
    }

    fn add(&mut self, region: usize, handle: Handle) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            handle,
            region,
            roots: 0,
            refs: [None; 2],
            live: true,
        });
        id
    }

    fn close(&mut self, region: usize) {
        assert!(self
            .live(region)
            .iter()
            .all(|&id| self.nodes[id].roots == 0));
        for node in &mut self.nodes {
            if node.region == region {
                node.live = false;
            }
        }
        self.stats[region].committed_bytes = 0;
        self.stats[region].live_bytes = 0;
        self.closed[region] = true;
    }
}

fn next(state: &mut u64, choices: usize) -> usize {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    ((*state >> 32) as usize) % choices
}

fn check(session: &Session, oracle: &Oracle, regions: [RegionId; 2], seed: u64, step: usize) {
    let report = session.memory_report();
    for (region, region_id) in regions.iter().copied().enumerate() {
        let row = report
            .regions
            .iter()
            .find(|row| row.id == region_id)
            .unwrap();
        assert_eq!(
            session.stats(region_id),
            if oracle.closed[region] {
                Err(MemoryError::InvalidRegion)
            } else {
                Ok(oracle.stats[region])
            },
            "seed {seed} step {step} region {region}"
        );
        assert_eq!(row.committed_bytes, oracle.stats[region].committed_bytes);
        assert_eq!(row.live_bytes, oracle.stats[region].live_bytes);
        assert_eq!(row.peak_bytes, oracle.stats[region].peak_bytes);
        assert_eq!(
            row.allocation_failures,
            oracle.stats[region].allocation_failures
        );
        assert_eq!(row.collection_count, oracle.stats[region].collection_count);
        assert_eq!(
            row.total_collected_bytes,
            oracle.stats[region].total_collected_bytes
        );
        assert_eq!(row.is_open, !oracle.closed[region]);
    }
    for (id, node) in oracle.nodes.iter().enumerate() {
        if node.live {
            assert_eq!(
                session.read_bytes(node.handle),
                Ok(&[0][..]),
                "seed {seed} step {step} node {id}"
            );
            for slot in 0..2 {
                let expected = node.refs[slot].map(|child| oracle.nodes[child].handle);
                assert_eq!(
                    session.reference(node.handle, slot),
                    Ok(expected),
                    "seed {seed} step {step} node {id} slot {slot}"
                );
            }
        } else {
            assert_eq!(
                session.read_bytes(node.handle),
                Err(MemoryError::InvalidHandle),
                "seed {seed} step {step} stale node {id}"
            );
        }
    }
}

fn allocate(
    session: &mut Session,
    oracle: &mut Oracle,
    regions: [RegionId; 2],
    region: usize,
    guarded: bool,
    fail_at: Option<usize>,
    trace: (u64, usize),
) -> Option<usize> {
    let (seed, step) = trace;
    if let Some(index) = fail_at {
        session.inject_host_failure_at_reserve(index);
    }
    let expected = oracle.before_allocation(region, guarded, fail_at.is_some());
    let actual = session.alloc_object(regions[region], 1, 2);
    match expected {
        Ok(()) => Some(oracle.add(
            region,
            actual.unwrap_or_else(|error| panic!("seed {seed} step {step}: {error:?}")),
        )),
        Err(error) => {
            assert_eq!(actual, Err(error), "seed {seed} step {step}");
            assert_eq!(session.host_reserve_attempts(), fail_at.unwrap());
            None
        }
    }
}

fn operation(
    session: &mut Session,
    oracle: &mut Oracle,
    regions: [RegionId; 2],
    state: &mut u64,
    guarded: bool,
    seed: u64,
    step: usize,
) {
    let region = next(state, 2);
    let live = oracle.live(region);
    match next(state, 10) {
        0 | 1 => {
            allocate(
                session,
                oracle,
                regions,
                region,
                guarded,
                None,
                (seed, step),
            );
        }
        2 => {
            let index = 1 + next(state, 2);
            allocate(
                session,
                oracle,
                regions,
                region,
                guarded,
                Some(index),
                (seed, step),
            );
            allocate(
                session,
                oracle,
                regions,
                region,
                guarded,
                None,
                (seed, step),
            );
        }
        3 if !live.is_empty() => {
            let id = live[next(state, live.len())];
            session.root(oracle.nodes[id].handle).unwrap();
            oracle.nodes[id].roots += 1;
        }
        4 if !live.is_empty() => {
            let rooted: Vec<_> = live
                .iter()
                .copied()
                .filter(|&id| oracle.nodes[id].roots > 0)
                .collect();
            if !rooted.is_empty() {
                let id = rooted[next(state, rooted.len())];
                session.unroot(oracle.nodes[id].handle).unwrap();
                oracle.nodes[id].roots -= 1;
            }
        }
        5 if live.len() > 1 => {
            let owner = live[next(state, live.len())];
            let child = live[next(state, live.len())];
            let slot = next(state, 2);
            session
                .set_reference(
                    oracle.nodes[owner].handle,
                    slot,
                    Some(oracle.nodes[child].handle),
                )
                .unwrap();
            oracle.nodes[owner].refs[slot] = Some(child);
        }
        6 if !live.is_empty() => {
            let owner = live[next(state, live.len())];
            let slot = next(state, 2);
            session
                .set_reference(oracle.nodes[owner].handle, slot, None)
                .unwrap();
            oracle.nodes[owner].refs[slot] = None;
        }
        7 if guarded => assert_eq!(
            session.collect(regions[region]),
            Err(MemoryError::CollectionForbidden)
        ),
        7 => {
            session.collect(regions[region]).unwrap();
            oracle.collect(region);
        }
        8 if !live.is_empty() => {
            let foreign = oracle.live(1 - region);
            if !foreign.is_empty() {
                let owner = live[next(state, live.len())];
                let child = foreign[next(state, foreign.len())];
                assert_eq!(
                    session.set_reference(
                        oracle.nodes[owner].handle,
                        0,
                        Some(oracle.nodes[child].handle)
                    ),
                    Err(MemoryError::CrossRegionEdge)
                );
            }
        }
        _ => {}
    }
    check(session, oracle, regions, seed, step);
}

#[test]
fn seeded_failure_graph_model_preserves_reachability_and_reports() {
    let mut aggregate = [0_u64; 3]; // manual collections, auto collections, allocation failures
    for seed in 0..SEEDS {
        let mut state = seed;
        let mut session = Session::new();
        let regions = [
            session
                .create_region(CollectionMode::Manual, 256 * CHARGE)
                .unwrap(),
            session
                .create_region(
                    CollectionMode::Auto {
                        trigger_bytes: 3 * CHARGE,
                    },
                    256 * CHARGE,
                )
                .unwrap(),
        ];
        let mut oracle = Oracle::new();
        // Force slot reuse and stale-handle checks before random operations.
        let stale = allocate(
            &mut session,
            &mut oracle,
            regions,
            0,
            false,
            None,
            (seed, 0),
        )
        .unwrap();
        session.collect(regions[0]).unwrap();
        oracle.collect(0);
        let replacement = allocate(
            &mut session,
            &mut oracle,
            regions,
            0,
            false,
            None,
            (seed, 1),
        )
        .unwrap();
        assert_ne!(oracle.nodes[stale].handle, oracle.nodes[replacement].handle);
        check(&session, &oracle, regions, seed, 1);

        // Overwrite and clear an edge, then reclaim an unreachable cycle.
        let a = replacement;
        let b = allocate(
            &mut session,
            &mut oracle,
            regions,
            0,
            false,
            None,
            (seed, 2),
        )
        .unwrap();
        let c = allocate(
            &mut session,
            &mut oracle,
            regions,
            0,
            false,
            None,
            (seed, 3),
        )
        .unwrap();
        session
            .set_reference(oracle.nodes[a].handle, 0, Some(oracle.nodes[b].handle))
            .unwrap();
        oracle.nodes[a].refs[0] = Some(b);
        session
            .set_reference(oracle.nodes[a].handle, 0, Some(oracle.nodes[c].handle))
            .unwrap();
        oracle.nodes[a].refs[0] = Some(c);
        session
            .set_reference(oracle.nodes[a].handle, 0, None)
            .unwrap();
        oracle.nodes[a].refs[0] = None;
        session
            .set_reference(oracle.nodes[b].handle, 0, Some(oracle.nodes[c].handle))
            .unwrap();
        oracle.nodes[b].refs[0] = Some(c);
        session
            .set_reference(oracle.nodes[c].handle, 0, Some(oracle.nodes[b].handle))
            .unwrap();
        oracle.nodes[c].refs[0] = Some(b);
        session.root(oracle.nodes[a].handle).unwrap();
        oracle.nodes[a].roots += 1;
        session.collect(regions[0]).unwrap();
        oracle.collect(0);
        check(&session, &oracle, regions, seed, 4);
        session.unroot(oracle.nodes[a].handle).unwrap();
        oracle.nodes[a].roots -= 1;

        for step in 0..OPERATIONS_PER_SEED {
            if step % 24 == 12 {
                let collections_before = [
                    oracle.stats[0].collection_count,
                    oracle.stats[1].collection_count,
                ];
                let mut outer = session.no_gc(128 * CHARGE);
                let mut inner = outer.no_gc(64 * CHARGE);
                for index in 1..=2 {
                    allocate(
                        &mut inner,
                        &mut oracle,
                        regions,
                        1,
                        true,
                        Some(index),
                        (seed, step),
                    );
                    check(&inner, &oracle, regions, seed, step);
                }
                operation(
                    &mut inner,
                    &mut oracle,
                    regions,
                    &mut state,
                    true,
                    seed,
                    step,
                );
                drop(inner);
                operation(
                    &mut outer,
                    &mut oracle,
                    regions,
                    &mut state,
                    true,
                    seed,
                    step,
                );
                drop(outer);
                assert_eq!(oracle.stats[0].collection_count, collections_before[0]);
                assert_eq!(oracle.stats[1].collection_count, collections_before[1]);
            } else {
                operation(
                    &mut session,
                    &mut oracle,
                    regions,
                    &mut state,
                    false,
                    seed,
                    step,
                );
            }
        }

        // Region closure has no collection side effect and cannot invalidate other regions.
        for region in 0..2 {
            for id in oracle.live(region) {
                while oracle.nodes[id].roots > 0 {
                    session.unroot(oracle.nodes[id].handle).unwrap();
                    oracle.nodes[id].roots -= 1;
                }
            }
            let other = 1 - region;
            let other_before = oracle.stats[other];
            session.close_region(regions[region]).unwrap();
            oracle.close(region);
            assert_eq!(oracle.stats[other], other_before);
            check(
                &session,
                &oracle,
                regions,
                seed,
                OPERATIONS_PER_SEED + region,
            );
        }
        aggregate[0] += oracle.stats[0].collection_count;
        aggregate[1] += oracle.stats[1].collection_count;
        aggregate[2] += oracle.stats[0].allocation_failures + oracle.stats[1].allocation_failures;
    }
    eprintln!(
        "graph failure model: seeds=0..{SEEDS}, scheduled_steps_per_seed={OPERATIONS_PER_SEED}, random_operations_per_seed={RANDOM_OPERATIONS_PER_SEED}, manual_collections={}, auto_collections={}, allocation_failures={}",
        aggregate[0], aggregate[1], aggregate[2]
    );
}
