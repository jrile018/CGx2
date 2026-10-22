use cgx2_runtime::{
    AllocationError, CollectionMode, Handle, MemoryError, RegionId, RegionStats, Session,
    MAX_OBJECT_REFERENCE_SLOTS, REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES,
};

const SEEDS: u64 = 128;
const STEPS: usize = 80;
const CHARGE: usize = SLOT_METADATA_BYTES + 1 + 2 * REFERENCE_SLOT_METADATA_BYTES;

struct Node {
    handle: Handle,
    region: usize,
    roots: usize,
    refs: [Option<usize>; 2],
    live: bool,
}

struct Model {
    nodes: Vec<Node>,
    stats: [RegionStats; 2],
}

impl Model {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            stats: [RegionStats::default(); 2],
        }
    }

    fn collect(&mut self, region: usize) {
        let mut marked = vec![false; self.nodes.len()];
        let mut stack: Vec<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.live && node.region == region && node.roots > 0)
            .map(|(index, _)| index)
            .collect();
        while let Some(index) = stack.pop() {
            if marked[index] {
                continue;
            }
            marked[index] = true;
            stack.extend(self.nodes[index].refs.into_iter().flatten());
        }
        for (index, node) in self.nodes.iter_mut().enumerate() {
            if node.live && node.region == region && !marked[index] {
                node.live = false;
                self.stats[region].committed_bytes -= CHARGE;
                self.stats[region].live_bytes -= CHARGE;
                self.stats[region].total_collected_bytes += CHARGE;
            }
        }
        self.stats[region].collection_count += 1;
    }

    fn allocate(&mut self, region: usize, mode: CollectionMode, limit: usize) -> bool {
        if let CollectionMode::Auto { trigger_bytes } = mode {
            if self.stats[region].committed_bytes >= trigger_bytes
                || CHARGE > limit.saturating_sub(self.stats[region].committed_bytes)
            {
                self.collect(region);
            }
        }
        if CHARGE > limit.saturating_sub(self.stats[region].committed_bytes) {
            self.stats[region].allocation_failures += 1;
            return false;
        }
        self.stats[region].committed_bytes += CHARGE;
        self.stats[region].live_bytes += CHARGE;
        self.stats[region].peak_bytes = self.stats[region]
            .peak_bytes
            .max(self.stats[region].committed_bytes);
        true
    }
}

fn choice(state: &mut u64, count: usize) -> usize {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    ((*state >> 32) as usize) % count
}

fn live_nodes(model: &Model, region: usize) -> Vec<usize> {
    model
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.live && node.region == region)
        .map(|(index, _)| index)
        .collect()
}

fn assert_state(session: &Session, model: &Model, regions: [RegionId; 2], seed: u64, step: usize) {
    let report = session.memory_report();
    for (index, region) in regions.into_iter().enumerate() {
        let actual = session.stats(region).unwrap();
        assert_eq!(
            actual, model.stats[index],
            "seed {seed} step {step} region {index}"
        );
        let row = report.regions.iter().find(|row| row.id == region).unwrap();
        assert_eq!(row.committed_bytes, actual.committed_bytes);
        assert_eq!(row.live_bytes, actual.live_bytes);
        assert_eq!(row.collection_count, actual.collection_count);
        assert_eq!(row.allocation_failures, actual.allocation_failures);
    }
    for (index, node) in model.nodes.iter().enumerate() {
        if node.live {
            assert_eq!(
                session.read_bytes(node.handle),
                Ok(&[0][..]),
                "seed {seed} step {step} node {index}"
            );
            for slot in 0..2 {
                let expected = node.refs[slot].map(|child| model.nodes[child].handle);
                assert_eq!(
                    session.reference(node.handle, slot),
                    Ok(expected),
                    "seed {seed} step {step} node {index} slot {slot}"
                );
            }
        } else {
            assert_eq!(
                session.read_bytes(node.handle),
                Err(MemoryError::InvalidHandle),
                "seed {seed} step {step} node {index}"
            );
        }
    }
}

#[test]
fn deterministic_graph_state_machine_matches_regions_and_reports() {
    for seed in 0..SEEDS {
        let mut state = seed;
        let mut session = Session::new();
        let modes = [
            CollectionMode::Manual,
            CollectionMode::Auto {
                trigger_bytes: 3 * CHARGE,
            },
        ];
        let limits = [5 * CHARGE, 6 * CHARGE];
        let regions = [
            session.create_region(modes[0], limits[0]).unwrap(),
            session.create_region(modes[1], limits[1]).unwrap(),
        ];
        let mut model = Model::new();
        for step in 0..STEPS {
            let region = choice(&mut state, 2);
            let live = live_nodes(&model, region);
            let op = choice(&mut state, 10);
            match op {
                0 | 1 => {
                    let expected = model.allocate(region, modes[region], limits[region]);
                    let result = session.alloc_object(regions[region], 1, 2);
                    if expected {
                        let handle = result.unwrap();
                        model.nodes.push(Node {
                            handle,
                            region,
                            roots: 0,
                            refs: [None; 2],
                            live: true,
                        });
                    } else {
                        assert_eq!(
                            result,
                            Err(AllocationError::RegionLimit),
                            "seed {seed} step {step}"
                        );
                    }
                }
                2 if !live.is_empty() => {
                    let index = live[choice(&mut state, live.len())];
                    session.root(model.nodes[index].handle).unwrap();
                    model.nodes[index].roots += 1;
                }
                3 if !live.is_empty() => {
                    let rooted: Vec<_> = live
                        .into_iter()
                        .filter(|&index| model.nodes[index].roots > 0)
                        .collect();
                    if !rooted.is_empty() {
                        let index = rooted[choice(&mut state, rooted.len())];
                        session.unroot(model.nodes[index].handle).unwrap();
                        model.nodes[index].roots -= 1;
                    }
                }
                4 if !live.is_empty() => {
                    let owner = live[choice(&mut state, live.len())];
                    let child = live[choice(&mut state, live.len())];
                    let slot = choice(&mut state, 2);
                    session
                        .set_reference(
                            model.nodes[owner].handle,
                            slot,
                            Some(model.nodes[child].handle),
                        )
                        .unwrap();
                    model.nodes[owner].refs[slot] = Some(child);
                }
                5 if !live.is_empty() => {
                    let owner = live[choice(&mut state, live.len())];
                    let slot = choice(&mut state, 2);
                    session
                        .set_reference(model.nodes[owner].handle, slot, None)
                        .unwrap();
                    model.nodes[owner].refs[slot] = None;
                }
                6 => {
                    session.collect(regions[region]).unwrap();
                    model.collect(region);
                }
                7 if !live.is_empty() => {
                    let owner = live[choice(&mut state, live.len())];
                    assert_eq!(
                        session.set_reference(model.nodes[owner].handle, 2, None),
                        Err(MemoryError::InvalidReferenceSlot)
                    );
                }
                8 => {
                    assert_eq!(
                        session.alloc_object(regions[region], 1, MAX_OBJECT_REFERENCE_SLOTS + 1),
                        Err(AllocationError::ShapeLimit)
                    );
                    model.stats[region].allocation_failures += 1;
                }
                9 if !live.is_empty() => {
                    let foreign = live_nodes(&model, 1 - region);
                    if !foreign.is_empty() {
                        let owner = live[choice(&mut state, live.len())];
                        let target = foreign[choice(&mut state, foreign.len())];
                        assert_eq!(
                            session.set_reference(
                                model.nodes[owner].handle,
                                0,
                                Some(model.nodes[target].handle)
                            ),
                            Err(MemoryError::CrossRegionEdge)
                        );
                    }
                }
                _ => {}
            }
            assert_state(&session, &model, regions, seed, step);
        }
    }
}
