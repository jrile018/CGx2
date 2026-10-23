//! Bounded region-graph oracle. Replay with `cargo test -p cgx2-memory-conformance
//! --test region_graph_wave22 --offline -- --nocapture`.
//! Logical nodes, reachability, and charges are computed without inspecting runtime slots.
use cgx2_runtime::{
    AllocationError, CollectionMode, Handle, MemoryError, RegionId, RegionStats, RootCensus,
    Session, EDGE_METADATA_BYTES, REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES,
};

const SEEDS: &str = include_str!("../corpus/wave22-region-graph-seeds.csv");
const OBJECT_CHARGE: usize = SLOT_METADATA_BYTES + 3 + REFERENCE_SLOT_METADATA_BYTES;
const BYTE_CHARGE: usize = SLOT_METADATA_BYTES + 3;

#[derive(Clone)]
struct Node {
    handle: Handle,
    region: usize,
    bytes: Vec<u8>,
    object: bool,
    charge: usize,
    roots: usize,
    child: Option<usize>,
    legacy_child: Option<usize>,
    live: bool,
}

struct Oracle {
    nodes: Vec<Node>,
    stats: [RegionStats; 2],
    closed: [bool; 2],
    first_error: Option<String>,
}

impl Oracle {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            stats: [RegionStats::default(); 2],
            closed: [false; 2],
            first_error: None,
        }
    }

    fn add(&mut self, handle: Handle, region: usize, object: bool, bytes: Vec<u8>) -> usize {
        let charge = if object { OBJECT_CHARGE } else { BYTE_CHARGE };
        let stats = &mut self.stats[region];
        stats.committed_bytes += charge;
        stats.live_bytes += charge;
        stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
        let id = self.nodes.len();
        self.nodes.push(Node {
            handle,
            region,
            bytes,
            object,
            charge,
            roots: 0,
            child: None,
            legacy_child: None,
            live: true,
        });
        id
    }

    fn fail(&mut self, error: impl std::fmt::Debug) {
        self.first_error.get_or_insert_with(|| format!("{error:?}"));
    }

    fn collect(&mut self, region: usize) {
        let mut reachable = vec![false; self.nodes.len()];
        let mut pending: Vec<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, node)| {
                (node.live && node.region == region && node.roots > 0).then_some(id)
            })
            .collect();
        while let Some(id) = pending.pop() {
            if reachable[id] {
                continue;
            }
            reachable[id] = true;
            if let Some(child) = self.nodes[id].child {
                pending.push(child);
            }
            if let Some(child) = self.nodes[id].legacy_child {
                pending.push(child);
            }
        }
        for (id, node) in self.nodes.iter_mut().enumerate() {
            if node.live && node.region == region && !reachable[id] {
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
            if node.live && region.is_none_or(|r| node.region == r) && node.roots > 0 {
                census.rooted_handles += 1;
                census.root_registrations += node.roots;
            }
        }
        census
    }
}

struct Runner {
    session: Session,
    regions: [RegionId; 2],
    oracle: Oracle,
    seed: u64,
    trace: Vec<String>,
}

impl Runner {
    fn new(seed: u64) -> Self {
        let mut session = Session::new();
        let modes = if seed & 1 == 0 {
            [
                CollectionMode::Manual,
                CollectionMode::Auto {
                    trigger_bytes: 4096,
                },
            ]
        } else {
            [
                CollectionMode::Auto {
                    trigger_bytes: 4096,
                },
                CollectionMode::Manual,
            ]
        };
        let regions = [
            session.create_region(modes[0], 4096).unwrap(),
            session.create_region(modes[1], 4096).unwrap(),
        ];
        Self {
            session,
            regions,
            oracle: Oracle::new(),
            seed,
            trace: Vec::new(),
        }
    }

    fn step(&mut self, operation: impl Into<String>) {
        self.trace.push(operation.into());
        let context = || format!("seed=0x{:x}\n{}", self.seed, self.trace.join("\n"));
        let report = self.session.memory_report();
        let mut totals = RegionStats::default();
        assert_eq!(
            self.session.root_census(),
            Ok(self.oracle.census(None)),
            "{}",
            context()
        );
        for r in 0..2 {
            let expected = self.oracle.stats[r];
            let region = self.regions[r];
            assert_eq!(
                self.session.stats(region),
                if self.oracle.closed[r] {
                    Err(MemoryError::InvalidRegion)
                } else {
                    Ok(expected)
                },
                "{}",
                context()
            );
            assert_eq!(
                self.session.root_census_region(region),
                if self.oracle.closed[r] {
                    Err(MemoryError::InvalidRegion)
                } else {
                    Ok(self.oracle.census(Some(r)))
                },
                "{}",
                context()
            );
            let row = report.regions.iter().find(|row| row.id == region).unwrap();
            assert_eq!(row.is_open, !self.oracle.closed[r], "{}", context());
            assert_eq!(
                row.committed_bytes,
                expected.committed_bytes,
                "{}",
                context()
            );
            assert_eq!(row.live_bytes, expected.live_bytes, "{}", context());
            assert_eq!(row.peak_bytes, expected.peak_bytes, "{}", context());
            assert_eq!(
                row.allocation_failures,
                expected.allocation_failures,
                "{}",
                context()
            );
            assert_eq!(
                row.collection_count,
                expected.collection_count,
                "{}",
                context()
            );
            assert_eq!(
                row.total_collected_bytes,
                expected.total_collected_bytes,
                "{}",
                context()
            );
            totals.committed_bytes += expected.committed_bytes;
            totals.live_bytes += expected.live_bytes;
            totals.peak_bytes += expected.peak_bytes;
            totals.allocation_failures += expected.allocation_failures;
            totals.collection_count += expected.collection_count;
            totals.total_collected_bytes += expected.total_collected_bytes;
        }
        assert_eq!(report.totals, totals, "{}", context());
        for (id, node) in self.oracle.nodes.iter().enumerate() {
            if node.live {
                assert_eq!(
                    self.session.read_bytes(node.handle),
                    Ok(node.bytes.as_slice()),
                    "node {id} {}",
                    context()
                );
                if node.object {
                    assert_eq!(
                        self.session.reference(node.handle, 0),
                        Ok(node.child.map(|child| self.oracle.nodes[child].handle)),
                        "node {id} {}",
                        context()
                    );
                }
            } else {
                assert_eq!(
                    self.session.read_bytes(node.handle),
                    Err(MemoryError::InvalidHandle),
                    "stale node {id} {}",
                    context()
                );
            }
        }
    }

    fn object(&mut self, region: usize) -> usize {
        let handle = self
            .session
            .alloc_object(self.regions[region], 3, 1)
            .unwrap();
        let id = self.oracle.add(handle, region, true, vec![0; 3]);
        self.step(format!("object r{region} -> n{id}"));
        id
    }

    fn root(&mut self, id: usize) {
        self.session.root(self.oracle.nodes[id].handle).unwrap();
        self.oracle.nodes[id].roots += 1;
        self.step(format!("root n{id}"));
    }

    fn unroot(&mut self, id: usize) {
        self.session.unroot(self.oracle.nodes[id].handle).unwrap();
        self.oracle.nodes[id].roots -= 1;
        self.step(format!("unroot n{id}"));
    }

    fn set_child(&mut self, parent: usize, child: Option<usize>) {
        let handle = child.map(|id| self.oracle.nodes[id].handle);
        self.session
            .set_reference(self.oracle.nodes[parent].handle, 0, handle)
            .unwrap();
        self.oracle.nodes[parent].child = child;
        self.step(format!("set_reference n{parent} -> {child:?}"));
    }

    fn link(&mut self, parent: usize, child: usize) {
        self.session
            .link(
                self.oracle.nodes[parent].handle,
                self.oracle.nodes[child].handle,
            )
            .unwrap();
        self.oracle.nodes[parent].legacy_child = Some(child);
        self.oracle.nodes[parent].charge += EDGE_METADATA_BYTES;
        let region = self.oracle.nodes[parent].region;
        let stats = &mut self.oracle.stats[region];
        stats.committed_bytes += EDGE_METADATA_BYTES;
        stats.live_bytes += EDGE_METADATA_BYTES;
        stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
        self.step(format!("link n{parent} -> n{child}"));
    }

    fn collect(&mut self, region: usize) {
        self.session.collect(self.regions[region]).unwrap();
        self.oracle.collect(region);
        self.step(format!("collect r{region}"));
    }

    fn close(&mut self, region: usize) {
        self.session.close_region(self.regions[region]).unwrap();
        self.oracle.close(region);
        self.step(format!("close r{region}"));
    }
}

fn replay(seed: u64) {
    let mut run = Runner::new(seed);
    run.step("create manual/auto regions");
    let source = (seed as usize >> 1) & 1;
    let target = 1 - source;
    let parent = run.object(source);
    let child = run.object(source);
    let foreign = run.object(target);
    run.root(parent);
    run.root(parent); // Duplicate root registrations are independent.
    run.root(foreign);
    run.set_child(parent, Some(child));
    run.link(parent, child);
    run.set_child(parent, None); // Legacy link alone now retains the child.
    let before = run.session.memory_report();
    let illegal = run.session.set_reference(
        run.oracle.nodes[parent].handle,
        0,
        Some(run.oracle.nodes[foreign].handle),
    );
    assert_eq!(illegal, Err(MemoryError::CrossRegionEdge));
    run.oracle.fail(MemoryError::CrossRegionEdge);
    assert_eq!(run.session.memory_report(), before);
    run.step("reject cross-region reference, preserve existing edge");

    // An independent owned copy is mutated before promotion. The original must stay zero.
    let mut owned = run
        .session
        .copy_managed_to_owned(run.oracle.nodes[parent].handle)
        .unwrap();
    owned.as_mut_slice()[0] = (seed as u8) | 1;
    let copied_bytes = owned.as_slice().to_vec();
    let copy = run
        .session
        .promote_owned(run.regions[target], owned)
        .unwrap();
    let copy_id = run.oracle.add(copy, target, false, copied_bytes);
    assert_ne!(
        run.session.read_bytes(copy).unwrap().as_ptr(),
        run.session
            .read_bytes(run.oracle.nodes[parent].handle)
            .unwrap()
            .as_ptr()
    );
    run.step(format!(
        "copy n{parent} through owned into r{target} -> n{copy_id}"
    ));
    run.root(copy_id);
    run.collect(source); // Child survives through the reference.

    let garbage = run.object(source);
    run.collect(source); // Unrooted allocation dies; its handle stays stale.
    let replacement = run.object(source);
    assert_ne!(
        run.oracle.nodes[garbage].handle,
        run.oracle.nodes[replacement].handle
    );
    run.step(format!("new generation after collecting n{garbage}"));

    let before = run.session.memory_report();
    assert_eq!(
        run.session.root(run.oracle.nodes[garbage].handle),
        Err(MemoryError::InvalidHandle)
    );
    run.oracle.fail(MemoryError::InvalidHandle);
    assert_eq!(run.session.memory_report(), before);
    run.step(format!("reject stale root n{garbage}"));

    let before = run.session.memory_report();
    assert_eq!(
        run.session.close_region(run.regions[source]),
        Err(MemoryError::RegionHasRoots)
    );
    run.oracle.fail(MemoryError::RegionHasRoots);
    assert_eq!(run.session.memory_report(), before);
    run.step(format!("reject rooted close r{source}"));
    run.unroot(parent);
    run.unroot(parent);
    run.close(source);
    run.step(format!("copied n{copy_id} survives source close"));
    run.unroot(foreign);
    run.unroot(copy_id);
    run.close(target);
    assert_eq!(run.oracle.first_error.as_deref(), Some("CrossRegionEdge"));
}

#[test]
fn deterministic_region_graph_replay() {
    let requested = std::env::var("CGX2_WAVE22_SEED").ok();
    let seeds: Vec<&str> = requested
        .as_deref()
        .map(|seed| vec![seed])
        .unwrap_or_else(|| SEEDS.lines().skip(1).collect());
    for line in seeds {
        let seed = u64::from_str_radix(line.trim_start_matches("0x"), 16).unwrap();
        replay(seed);
    }
}

#[test]
fn injected_oracle_mismatch_is_detected() {
    let mut run = Runner::new(0x22);
    let id = run.object(0);
    run.oracle.nodes[id].bytes[0] = 1; // Deliberately wrong expectation.
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run.step("injected mismatch")
    }));
    assert!(caught.is_err(), "oracle silently accepted a wrong byte");
}

#[test]
fn nested_guard_exhaustion_retry_and_failed_link_then_close() {
    let mut run = Runner::new(0x23);
    let parent = run.object(0);
    let foreign = run.object(1);
    run.root(parent);
    run.root(foreign);
    {
        let mut outer = run.session.no_gc(256);
        {
            let mut inner = outer.no_gc(0);
            assert_eq!(
                inner.alloc_bytes(run.regions[0], 3),
                Err(AllocationError::BudgetExceeded)
            );
        }
        let retry = outer.alloc_bytes(run.regions[0], 3).unwrap();
        let id = run.oracle.add(retry, 0, false, vec![0; 3]);
        run.oracle.stats[0].allocation_failures += 1;
        run.oracle.fail(AllocationError::BudgetExceeded);
        drop(outer);
        run.step(format!("nested budget failure then retry -> n{id}"));
    }
    assert_eq!(
        run.session.set_reference(
            run.oracle.nodes[parent].handle,
            0,
            Some(run.oracle.nodes[foreign].handle)
        ),
        Err(MemoryError::CrossRegionEdge)
    );
    run.step("failed cross-region link before close");
    run.unroot(parent);
    run.close(0);
    run.step("unrelated region survives failed link and source close");
    run.unroot(foreign);
    run.close(1);
}

#[test]
#[cfg(feature = "fault-injection")]
fn reserve_failures_at_each_allocation_site_preserve_both_regions() {
    for index in 1..=3 {
        let mut run = Runner::new(index as u64);
        let survivor = run.object(1);
        run.root(survivor);
        run.session.inject_host_failure_at_reserve(index);
        assert_eq!(
            run.session.alloc_object(run.regions[0], 3, 1),
            Err(AllocationError::HostAllocation),
            "reserve index {index}"
        );
        assert_eq!(run.session.host_reserve_attempts(), index);
        run.oracle.stats[0].allocation_failures += 1;
        run.oracle.fail(AllocationError::HostAllocation);
        run.step(format!("host reserve failure at index {index}"));
        run.session.disable_host_failure();
        let retry = run.object(0);
        run.root(retry);
        run.unroot(retry);
        run.close(0);
        run.unroot(survivor);
        run.close(1);
    }
}
