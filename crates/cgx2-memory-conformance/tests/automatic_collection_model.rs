//! Bounded release corpus for auto/manual safepoints against logical node IDs.
use cgx2_runtime::{
    AllocationError, CollectionMode, Handle, MemoryError, RegionId, RegionStats, Session,
    REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES,
};
use std::time::{Duration, Instant};

const SEEDS: u64 = 1024;
const RANDOM_STEPS: usize = 96;
const MAX_NODES: usize = 160;
const STEPS_PER_SEED: usize = 23 + RANDOM_STEPS + 6;
const MAX_TRACE: usize = STEPS_PER_SEED;
const MAX_TEST_TIME: Duration = Duration::from_secs(30);
const CHARGE: usize = SLOT_METADATA_BYTES + 1 + 2 * REFERENCE_SLOT_METADATA_BYTES;
const PROMOTED_CHARGE: usize = SLOT_METADATA_BYTES + 1;
const TRIGGER: usize = 3 * CHARGE;
const LIMIT: usize = MAX_NODES * CHARGE;

#[derive(Clone, Copy, Debug)]
enum Op {
    Alloc(usize),
    Root(usize),
    Unroot(usize),
    Set(usize, usize, Option<usize>),
    Promote(usize),
    Copy(usize),
    Collect(usize),
    NestedNoGc(usize),
    DrainRoots(usize),
    Close(usize),
}

struct Node {
    handle: Handle,
    region: usize,
    roots: usize,
    refs: Option<[Option<usize>; 2]>,
    charge: usize,
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

    fn collect(&mut self, region: usize) {
        let mut reached = vec![false; self.nodes.len()];
        let mut pending: Vec<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, node)| {
                (node.live && node.region == region && node.roots > 0).then_some(id)
            })
            .collect();
        while let Some(id) = pending.pop() {
            if reached[id] {
                continue;
            }
            reached[id] = true;
            if let Some(refs) = self.nodes[id].refs {
                pending.extend(refs.into_iter().flatten());
            }
        }
        for (id, node) in self.nodes.iter_mut().enumerate() {
            if node.live && node.region == region && !reached[id] {
                node.live = false;
                self.stats[region].committed_bytes -= node.charge;
                self.stats[region].live_bytes -= node.charge;
                self.stats[region].total_collected_bytes += node.charge;
            }
        }
        self.stats[region].collection_count += 1;
    }

    fn check(&self, session: &mut Session, regions: [RegionId; 2]) -> Result<(), String> {
        let report = session.memory_report();
        for (region, id) in regions.into_iter().enumerate() {
            let expected = if self.closed[region] {
                Err(MemoryError::InvalidRegion)
            } else {
                Ok(self.stats[region])
            };
            equal(session.stats(id), expected)?;
            let row = report.regions.iter().find(|row| row.id == id).unwrap();
            equal(row.is_open, !self.closed[region])?;
            equal(row.committed_bytes, self.stats[region].committed_bytes)?;
            equal(row.live_bytes, self.stats[region].live_bytes)?;
            equal(row.peak_bytes, self.stats[region].peak_bytes)?;
            equal(
                row.allocation_failures,
                self.stats[region].allocation_failures,
            )?;
            equal(row.collection_count, self.stats[region].collection_count)?;
            equal(row.pause.sample_count, self.stats[region].collection_count)?;
            equal(
                row.total_collected_bytes,
                self.stats[region].total_collected_bytes,
            )?;
        }
        equal(
            report.totals.committed_bytes,
            self.stats.iter().map(|s| s.committed_bytes).sum(),
        )?;
        equal(
            report.totals.live_bytes,
            self.stats.iter().map(|s| s.live_bytes).sum(),
        )?;
        equal(
            report.totals.peak_bytes,
            self.stats.iter().map(|s| s.peak_bytes).sum(),
        )?;
        equal(
            report.totals.allocation_failures,
            self.stats.iter().map(|s| s.allocation_failures).sum(),
        )?;
        equal(
            report.totals.collection_count,
            self.stats.iter().map(|s| s.collection_count).sum(),
        )?;
        equal(
            report.totals.total_collected_bytes,
            self.stats.iter().map(|s| s.total_collected_bytes).sum(),
        )?;
        if report.totals.committed_bytes > 2 * LIMIT {
            return Err("logical committed bytes exceeded the two-region bound".into());
        }
        for (id, node) in self.nodes.iter().enumerate() {
            if node.live {
                equal(session.read_bytes(node.handle), Ok(&[0][..]))
                    .map_err(|error| format!("node {id}: {error}"))?;
                // Root counts are not exposed by the public API. Drain and restore them
                // without allocating or collecting so every transition checks the count.
                for _ in 0..node.roots {
                    equal(session.unroot(node.handle), Ok(()))
                        .map_err(|error| format!("node {id} root count: {error}"))?;
                }
                equal(session.unroot(node.handle), Err(MemoryError::NotRooted))
                    .map_err(|error| format!("node {id} extra root: {error}"))?;
                for _ in 0..node.roots {
                    equal(session.root(node.handle), Ok(()))
                        .map_err(|error| format!("node {id} root restore: {error}"))?;
                }
                if let Some(refs) = node.refs {
                    for (slot, target) in refs.into_iter().enumerate() {
                        let expected = target.map(|child| self.nodes[child].handle);
                        equal(session.reference(node.handle, slot), Ok(expected))
                            .map_err(|error| format!("node {id} slot {slot}: {error}"))?;
                    }
                } else {
                    equal(
                        session.reference(node.handle, 0),
                        Err(MemoryError::NotObject),
                    )?;
                }
            } else {
                equal(
                    session.read_bytes(node.handle),
                    Err(MemoryError::InvalidHandle),
                )
                .map_err(|error| format!("stale node {id}: {error}"))?;
            }
        }
        Ok(())
    }
}

fn equal<T: std::fmt::Debug + PartialEq>(actual: T, expected: T) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {actual:?}"))
    }
}

fn apply(
    op: Op,
    session: &mut Session,
    model: &mut Model,
    regions: [RegionId; 2],
) -> Result<(), String> {
    match op {
        Op::Alloc(region) => {
            if model.nodes.len() >= MAX_NODES {
                return Ok(());
            }
            if region == 1 && model.stats[region].committed_bytes >= TRIGGER {
                model.collect(region);
            }
            let result = session.alloc_object(regions[region], 1, 2);
            if model.closed[region] {
                equal(result, Err(AllocationError::InvalidRegion))?;
            } else {
                let handle = result.map_err(|error| format!("allocation: {error:?}"))?;
                model.nodes.push(Node {
                    handle,
                    region,
                    roots: 0,
                    refs: Some([None; 2]),
                    charge: CHARGE,
                    live: true,
                });
                let stats = &mut model.stats[region];
                stats.committed_bytes += CHARGE;
                stats.live_bytes += CHARGE;
                stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
            }
        }
        Op::Root(id) => {
            if let Some(node) = model.nodes.get_mut(id) {
                let result = session.root(node.handle);
                if node.live {
                    equal(result, Ok(()))?;
                    node.roots += 1;
                } else {
                    equal(result, Err(MemoryError::InvalidHandle))?;
                }
            }
        }
        Op::Unroot(id) => {
            if let Some(node) = model.nodes.get_mut(id) {
                let result = session.unroot(node.handle);
                if !node.live {
                    equal(result, Err(MemoryError::InvalidHandle))?;
                } else if node.roots == 0 {
                    equal(result, Err(MemoryError::NotRooted))?;
                } else {
                    equal(result, Ok(()))?;
                    node.roots -= 1;
                }
            }
        }
        Op::Set(owner, slot, target) => {
            if let Some(node) = model.nodes.get(owner) {
                if target.is_some_and(|id| id >= model.nodes.len()) {
                    return Ok(());
                }
                let target_handle = target.map(|id| model.nodes[id].handle);
                let expected = if !node.live {
                    Err(MemoryError::InvalidHandle)
                } else if node.refs.is_none() {
                    Err(MemoryError::NotObject)
                } else if let Some(id) = target {
                    if !model.nodes[id].live {
                        Err(MemoryError::InvalidHandle)
                    } else if node.region != model.nodes[id].region {
                        Err(MemoryError::CrossRegionEdge)
                    } else {
                        Ok(())
                    }
                } else {
                    Ok(())
                };
                equal(
                    session.set_reference(node.handle, slot, target_handle),
                    expected,
                )?;
                if expected.is_ok() {
                    model.nodes[owner].refs.as_mut().unwrap()[slot] = target;
                }
            }
        }
        Op::Promote(region) => {
            if model.nodes.len() >= MAX_NODES {
                return Ok(());
            }
            let value = session.alloc_owned_bytes(1).unwrap();
            let handle = session
                .promote_owned(regions[region], value)
                .map_err(|(error, _)| format!("promotion: {error:?}"))?;
            model.nodes.push(Node {
                handle,
                region,
                roots: 0,
                refs: None,
                charge: PROMOTED_CHARGE,
                live: true,
            });
            let stats = &mut model.stats[region];
            stats.committed_bytes += PROMOTED_CHARGE;
            stats.live_bytes += PROMOTED_CHARGE;
            stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
        }
        Op::Copy(id) => {
            if let Some(node) = model.nodes.get(id) {
                match session.copy_managed_to_owned(node.handle) {
                    Ok(value) if node.live => equal(value.as_slice(), &[0][..])?,
                    Err(MemoryError::InvalidHandle) if !node.live => {}
                    other => return Err(format!("copy node {id}: unexpected result {other:?}")),
                }
            }
        }
        Op::Collect(region) => {
            let expected = if model.closed[region] {
                Err(MemoryError::InvalidRegion)
            } else {
                Ok(())
            };
            equal(session.collect(regions[region]), expected)?;
            if expected.is_ok() {
                model.collect(region);
            }
        }
        Op::NestedNoGc(region) => {
            if !model.closed[region] {
                let counts = [
                    model.stats[0].collection_count,
                    model.stats[1].collection_count,
                ];
                let mut outer = session.no_gc(0);
                let mut inner = outer.no_gc(0);
                for &other in &regions {
                    equal(inner.collect(other), Err(MemoryError::CollectionForbidden))?;
                }
                equal(
                    inner.alloc_object(regions[region], 1, 2),
                    Err(AllocationError::BudgetExceeded),
                )?;
                model.stats[region].allocation_failures += 1;
                drop(inner);
                equal(
                    outer.alloc_object(regions[region], 1, 2),
                    Err(AllocationError::BudgetExceeded),
                )?;
                model.stats[region].allocation_failures += 1;
                drop(outer);
                equal(
                    [
                        session
                            .stats(regions[0])
                            .map_err(|e| format!("{e:?}"))?
                            .collection_count,
                        session
                            .stats(regions[1])
                            .map_err(|e| format!("{e:?}"))?
                            .collection_count,
                    ],
                    counts,
                )?;
            }
        }
        Op::DrainRoots(region) => {
            for node in &mut model.nodes {
                if node.live && node.region == region {
                    while node.roots > 0 {
                        equal(session.unroot(node.handle), Ok(()))?;
                        node.roots -= 1;
                    }
                }
            }
        }
        Op::Close(region) => {
            let expected = if model.closed[region] {
                Err(MemoryError::InvalidRegion)
            } else if model
                .nodes
                .iter()
                .any(|node| node.live && node.region == region && node.roots > 0)
            {
                Err(MemoryError::RegionHasRoots)
            } else {
                Ok(())
            };
            equal(session.close_region(regions[region]), expected)?;
            if expected.is_ok() {
                model.closed[region] = true;
                for node in &mut model.nodes {
                    if node.region == region {
                        node.live = false;
                    }
                }
                model.stats[region].committed_bytes = 0;
                model.stats[region].live_bytes = 0;
            }
        }
    }
    Ok(())
}

fn replay(trace: &[Op]) -> Result<(), String> {
    let mut session = Session::new();
    let regions = [
        session
            .create_region(CollectionMode::Manual, LIMIT)
            .unwrap(),
        session
            .create_region(
                CollectionMode::Auto {
                    trigger_bytes: TRIGGER,
                },
                LIMIT,
            )
            .unwrap(),
    ];
    let mut model = Model::new();
    for (step, &op) in trace.iter().enumerate() {
        apply(op, &mut session, &mut model, regions)
            .and_then(|()| model.check(&mut session, regions))
            .map_err(|error| format!("step {step} {op:?}: {error}"))?;
    }
    Ok(())
}

fn next(state: &mut u64, choices: usize) -> usize {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    ((*state >> 32) as usize) % choices
}

fn generate(seed: u64) -> Vec<Op> {
    let mut trace = vec![
        Op::Alloc(1),
        Op::Root(0),
        Op::Alloc(1),
        Op::Set(0, 0, Some(1)),
        Op::Alloc(1),
        Op::Set(1, 0, Some(2)),
        Op::Set(2, 0, Some(1)),
        Op::Alloc(1), // Threshold collects, but all three connected nodes survive.
        Op::Set(0, 0, None),
        Op::Alloc(1), // Next safepoint reclaims child/cycle and the unrooted fourth.
        Op::Copy(0),
        Op::Copy(1), // Stale handle.
        Op::Promote(1),
        Op::Alloc(0),
        Op::Root(6),
        Op::Alloc(0),
        Op::Set(6, 0, Some(7)),
        Op::Collect(0), // Manual region retains its child.
        Op::Set(6, 0, None),
        Op::Collect(0), // Cleared edge releases the child.
        Op::NestedNoGc(0),
        Op::NestedNoGc(1),
        Op::Close(0), // Roots forbid close.
    ];
    let mut state = seed;
    let mut ids = 8;
    for _ in 0..RANDOM_STEPS {
        let region = next(&mut state, 2);
        let op = match next(&mut state, 12) {
            0 | 1 => {
                ids += 1;
                Op::Alloc(region)
            }
            2 => {
                ids += 1;
                Op::Promote(region)
            }
            3 => Op::Root(next(&mut state, ids)),
            4 => Op::Unroot(next(&mut state, ids)),
            5 => Op::Set(
                next(&mut state, ids),
                next(&mut state, 2),
                Some(next(&mut state, ids)),
            ),
            6 => Op::Set(next(&mut state, ids), next(&mut state, 2), None),
            7 => Op::Collect(region),
            8 => Op::Copy(next(&mut state, ids)),
            _ => Op::NestedNoGc(region),
        };
        trace.push(op);
    }
    for region in 0..2 {
        trace.push(Op::DrainRoots(region));
        trace.push(Op::Close(region));
        trace.push(Op::Collect(region)); // Closed identity stays invalid.
    }
    trace
}

fn minimize(mut trace: Vec<Op>) -> Vec<Op> {
    let mut chunk = trace.len() / 2;
    while chunk > 0 {
        let mut start = 0;
        while start + chunk <= trace.len() {
            let mut candidate = trace.clone();
            candidate.drain(start..start + chunk);
            if replay(&candidate).is_err() {
                trace = candidate;
            } else {
                start += chunk;
            }
        }
        chunk /= 2;
    }
    trace
}

#[test]
fn automatic_collection_state_model_corpus() {
    let started = Instant::now();
    for seed in 0..SEEDS {
        assert!(
            started.elapsed() < MAX_TEST_TIME,
            "graph audit exceeded 30 seconds"
        );
        let trace = generate(seed);
        assert_eq!(trace.len(), STEPS_PER_SEED);
        if let Err(error) = replay(&trace) {
            let minimized = minimize(trace);
            panic!(
                "seed {seed}: {error}\nminimized trace ({} operations):\n{:#?}",
                minimized.len(),
                minimized
            );
        }
    }
    assert!(
        started.elapsed() < MAX_TEST_TIME,
        "graph audit exceeded 30 seconds"
    );
    println!(
"release auto/manual corpus: seeds=0..{SEEDS} steps_per_seed={STEPS_PER_SEED} total_steps={} max_nodes={MAX_NODES} object_charge={CHARGE} promoted_charge={PROMOTED_CHARGE} trigger={TRIGGER} region_limit={LIMIT} logical_committed_bound={} max_trace={MAX_TRACE} time_bound_seconds={}",
SEEDS as usize * STEPS_PER_SEED,
2 * LIMIT,
MAX_TEST_TIME.as_secs()
);
}
