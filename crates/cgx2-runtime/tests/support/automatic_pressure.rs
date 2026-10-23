//! Independent logical graph and allocation-safepoint model, shared by two suites.
#[cfg(feature = "fault-injection")]
use cgx2_runtime::ReserveSite;
use cgx2_runtime::{
    AllocationError as A, CollectionMode, Handle, MemoryError as M, RegionId, RegionStats,
    RootCensus, Session, EDGE_METADATA_BYTES, REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES,
};
use std::collections::{BTreeMap, BTreeSet};

const C: usize = SLOT_METADATA_BYTES + 1 + REFERENCE_SLOT_METADATA_BYTES;
const LIMIT: usize = 12 * C;
const SEEDS: &str = include_str!("../../corpus/wave23-pressure-seeds.txt");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    None,
    Payload,
    References,
    #[cfg(feature = "fault-injection")]
    Slot,
}
#[derive(Clone, Copy, Debug)]
enum Op {
    Alloc(usize, bool, usize, Fault),
    Root(usize),
    Unroot(usize),
    Set(usize, Option<usize>),
    Link(usize, usize, bool),
    Copy(usize, usize, bool),
    Collect(usize),
    Close(usize),
    Nested(usize),
}
impl Op {
    fn rust(self) -> String {
        match self {
            Self::Alloc(r, object, len, fault) => {
                format!("Op::Alloc({r}, {object}, {len}, Fault::{fault:?})")
            }
            _ => format!("Op::{self:?}"),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Alloc(..) => "alloc",
            Self::Root(..) => "root",
            Self::Unroot(..) => "unroot",
            Self::Set(..) => "set",
            Self::Link(..) => "link",
            Self::Copy(..) => "copy",
            Self::Collect(..) => "collect",
            Self::Close(..) => "close",
            Self::Nested(..) => "nested",
        }
    }
}
#[derive(Default, Debug)]
struct Coverage {
    transitions: BTreeMap<String, usize>,
    outcomes: BTreeMap<String, usize>,
    auto: usize,
    explicit: usize,
    reclaimed: usize,
    rooted_survivors: usize,
    reused: usize,
    pre_failure: usize,
    post_failure: usize,
    budget_failure: usize,
}
impl Coverage {
    fn merge(&mut self, other: &Self) {
        for (key, value) in &other.transitions {
            *self.transitions.entry(key.clone()).or_default() += value;
        }
        for (key, value) in &other.outcomes {
            *self.outcomes.entry(key.clone()).or_default() += value;
        }
        self.auto += other.auto;
        self.explicit += other.explicit;
        self.reclaimed += other.reclaimed;
        self.rooted_survivors += other.rooted_survivors;
        self.reused += other.reused;
        self.pre_failure += other.pre_failure;
        self.post_failure += other.post_failure;
        self.budget_failure += other.budget_failure;
    }
}
struct Node {
    region: usize,
    slot: usize,
    generation: u64,
    bytes: Vec<u8>,
    object: bool,
    child: Option<usize>,
    edges: BTreeSet<usize>,
    roots: usize,
    charge: usize,
    live: bool,
}
struct Model {
    nodes: Vec<Node>,
    slots: [Vec<(u64, Option<usize>)>; 2],
    stats: [RegionStats; 2],
    closed: [bool; 2],
    auto_region: usize,
    trigger: usize,
    budgets: Vec<usize>,
    first_failure: Option<String>,
    coverage: Coverage,
}
impl Model {
    fn outcome(&mut self, op: Op, result: impl std::fmt::Debug) {
        *self
            .coverage
            .outcomes
            .entry(format!("{}:{result:?}", op.name()))
            .or_default() += 1;
    }
    fn new(seed: u64) -> Self {
        Self {
            nodes: vec![],
            slots: [vec![], vec![]],
            stats: [RegionStats::default(); 2],
            closed: [false; 2],
            auto_region: (seed & 1) as usize,
            trigger: (2 + (seed % 3) as usize) * C,
            budgets: vec![],
            first_failure: None,
            coverage: Coverage::default(),
        }
    }
    fn charge(&mut self, r: usize, bytes: usize, budget_charge: usize) {
        self.stats[r].committed_bytes += bytes;
        self.stats[r].live_bytes += bytes;
        self.stats[r].peak_bytes = self.stats[r].peak_bytes.max(self.stats[r].committed_bytes);
        for remaining in &mut self.budgets {
            *remaining -= budget_charge;
        }
    }
    fn fail(&mut self, error: impl std::fmt::Debug) {
        self.first_failure
            .get_or_insert_with(|| format!("{error:?}"));
    }
    fn alloc_fail(&mut self, r: usize, error: A) -> Result<usize, A> {
        if !self.closed[r] {
            self.stats[r].allocation_failures += 1;
        }
        if error == A::BudgetExceeded {
            self.coverage.budget_failure += 1;
        }
        Err(error)
    }
    fn collect(&mut self, r: usize, automatic: bool) {
        let mut reached = BTreeSet::new();
        let mut pending: Vec<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, n)| (n.live && n.region == r && n.roots > 0).then_some(id))
            .collect();
        while let Some(id) = pending.pop() {
            if !reached.insert(id) {
                continue;
            }
            pending.extend(self.nodes[id].child);
            pending.extend(&self.nodes[id].edges);
        }
        self.coverage.rooted_survivors += reached
            .iter()
            .filter(|&&id| self.nodes[id].roots > 0)
            .count();
        for (id, n) in self.nodes.iter_mut().enumerate() {
            if n.live && n.region == r && !reached.contains(&id) {
                n.live = false;
                self.slots[r][n.slot] = (n.generation + 1, None);
                self.stats[r].committed_bytes -= n.charge;
                self.stats[r].live_bytes -= n.charge;
                self.stats[r].total_collected_bytes += n.charge;
                self.coverage.reclaimed += 1;
            }
        }
        self.stats[r].collection_count += 1;
        if automatic {
            self.coverage.auto += 1;
        } else {
            self.coverage.explicit += 1;
        }
    }
    fn alloc(
        &mut self,
        r: usize,
        object: bool,
        bytes: Vec<u8>,
        fault: Fault,
        promotion: bool,
    ) -> Result<usize, A> {
        if self.closed[r] {
            return Err(A::InvalidRegion);
        }
        let charge = SLOT_METADATA_BYTES
            + bytes.len()
            + if object {
                REFERENCE_SLOT_METADATA_BYTES
            } else {
                0
            };
        let budget_charge = if promotion {
            SLOT_METADATA_BYTES
        } else {
            charge
        };
        if charge > LIMIT {
            return self.alloc_fail(r, A::RegionLimit);
        }
        if self.budgets.iter().any(|&b| b < budget_charge) {
            return self.alloc_fail(r, A::BudgetExceeded);
        }
        let pre_limit = promotion || r != self.auto_region || !self.budgets.is_empty();
        if pre_limit && charge > LIMIT - self.stats[r].committed_bytes {
            return self.alloc_fail(r, A::RegionLimit);
        }
        // Object payload and checked-reference reserves precede the automatic safepoint.
        if object && matches!(fault, Fault::Payload | Fault::References) {
            self.coverage.pre_failure += 1;
            return self.alloc_fail(r, A::HostAllocation);
        }
        let collect = !promotion
            && self.budgets.is_empty()
            && r == self.auto_region
            && (self.stats[r].committed_bytes >= self.trigger
                || charge > LIMIT - self.stats[r].committed_bytes);
        if collect {
            self.collect(r, true);
        }
        if charge > LIMIT - self.stats[r].committed_bytes {
            return self.alloc_fail(r, A::RegionLimit);
        }
        if fault != Fault::None {
            if collect {
                self.coverage.post_failure += 1;
            } else {
                self.coverage.pre_failure += 1;
            }
            return self.alloc_fail(r, A::HostAllocation);
        }
        let slot = self.slots[r]
            .iter()
            .position(|(_, id)| id.is_none())
            .unwrap_or(self.slots[r].len());
        if slot == self.slots[r].len() {
            self.slots[r].push((0, None));
        }
        let generation = self.slots[r][slot].0;
        self.coverage.reused += usize::from(generation > 0);
        let id = self.nodes.len();
        self.slots[r][slot].1 = Some(id);
        self.nodes.push(Node {
            region: r,
            slot,
            generation,
            bytes,
            object,
            child: None,
            edges: BTreeSet::new(),
            roots: 0,
            charge,
            live: true,
        });
        // Promotion only charges its managed header against a no_gc budget.
        self.charge(r, charge, budget_charge);
        Ok(id)
    }
    fn valid(&self, id: usize) -> Result<(), M> {
        if self.nodes[id].live {
            Ok(())
        } else {
            Err(M::InvalidHandle)
        }
    }
    fn census(&self, r: Option<usize>) -> RootCensus {
        let mut out = RootCensus::default();
        for n in &self.nodes {
            if n.live && n.roots > 0 && r.is_none_or(|r| n.region == r) {
                out.rooted_handles += 1;
                out.root_registrations += n.roots;
            }
        }
        out
    }
    fn check(
        &self,
        session: &Session,
        regions: [RegionId; 2],
        handles: &[Handle],
    ) -> Result<(), String> {
        equal(session.root_census(), Ok(self.census(None)))?;
        let report = session.memory_report();
        let mut totals = RegionStats::default();
        for (r, region) in regions.into_iter().enumerate() {
            equal(
                session.stats(region),
                if self.closed[r] {
                    Err(M::InvalidRegion)
                } else {
                    Ok(self.stats[r])
                },
            )?;
            equal(
                session.root_census_region(region),
                if self.closed[r] {
                    Err(M::InvalidRegion)
                } else {
                    Ok(self.census(Some(r)))
                },
            )?;
            let row = report
                .regions
                .iter()
                .find(|row| row.id == region)
                .ok_or("missing report row")?;
            equal(row.is_open, !self.closed[r])?;
            equal(
                (
                    row.committed_bytes,
                    row.live_bytes,
                    row.peak_bytes,
                    row.allocation_failures,
                    row.collection_count,
                    row.total_collected_bytes,
                ),
                (
                    self.stats[r].committed_bytes,
                    self.stats[r].live_bytes,
                    self.stats[r].peak_bytes,
                    self.stats[r].allocation_failures,
                    self.stats[r].collection_count,
                    self.stats[r].total_collected_bytes,
                ),
            )?;
            equal(row.pause.sample_count, self.stats[r].collection_count)?;
            totals.committed_bytes += self.stats[r].committed_bytes;
            totals.live_bytes += self.stats[r].live_bytes;
            totals.peak_bytes += self.stats[r].peak_bytes;
            totals.allocation_failures += self.stats[r].allocation_failures;
            totals.collection_count += self.stats[r].collection_count;
            totals.total_collected_bytes += self.stats[r].total_collected_bytes;
        }
        equal(report.totals, totals)?;
        for (id, n) in self.nodes.iter().enumerate() {
            equal(handles[id].generation(), n.generation)?;
            equal(handles[id].region(), regions[n.region])?;
            equal(
                session.read_bytes(handles[id]),
                if n.live {
                    Ok(n.bytes.as_slice())
                } else {
                    Err(M::InvalidHandle)
                },
            )?;
            if n.live && n.object {
                equal(
                    session.reference(handles[id], 0),
                    Ok(n.child.map(|id| handles[id])),
                )?;
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
fn allocation_result(
    model: &mut Model,
    handles: &mut Vec<Handle>,
    actual: Result<Handle, A>,
    expected: Result<usize, A>,
) -> Result<(), String> {
    equal(
        actual.as_ref().map(|_| ()).map_err(|e| *e),
        expected.as_ref().map(|_| ()).map_err(|e| *e),
    )?;
    match expected {
        Ok(id) => {
            equal(id, handles.len())?;
            handles.push(actual.unwrap());
        }
        Err(e) => model.fail(e),
    }
    Ok(())
}
fn apply(
    op: Op,
    session: &mut Session,
    regions: [RegionId; 2],
    model: &mut Model,
    handles: &mut Vec<Handle>,
) -> Result<(), String> {
    match op {
        Op::Alloc(r, object, len, fault) => {
            #[cfg(feature = "fault-injection")]
            if fault != Fault::None {
                session.inject_host_failure_at_reserve(match fault {
                    Fault::References => 2,
                    Fault::Slot => {
                        if object {
                            3
                        } else {
                            2
                        }
                    }
                    _ => 1,
                });
            }
            let expected = model.alloc(r, object, vec![0; len], fault, false);
            let actual = if object {
                session.alloc_object(regions[r], len, 1)
            } else {
                session.alloc_bytes(regions[r], len)
            };
            #[cfg(feature = "fault-injection")]
            if fault != Fault::None {
                let site = match fault {
                    Fault::References => ReserveSite::ObjectReferences,
                    Fault::Slot => ReserveSite::SlotTable,
                    _ if object => ReserveSite::ObjectPayload,
                    _ => ReserveSite::ManagedBytes,
                };
                equal(
                    session.last_host_failure_site(),
                    (expected == Err(A::HostAllocation)).then_some(site),
                )?;
            }
            allocation_result(model, handles, actual, expected)?;
            model.outcome(op, expected.map(|_| ()));
            #[cfg(feature = "fault-injection")]
            if fault != Fault::None {
                session.disable_host_failure();
            }
        }
        Op::Root(id) | Op::Unroot(id) => {
            let mut expected = model.valid(id);
            if expected.is_ok() {
                if matches!(op, Op::Root(_)) {
                    model.nodes[id].roots += 1;
                } else if model.nodes[id].roots == 0 {
                    expected = Err(M::NotRooted);
                } else {
                    model.nodes[id].roots -= 1;
                }
            }
            let actual = if matches!(op, Op::Root(_)) {
                session.root(handles[id])
            } else {
                session.unroot(handles[id])
            };
            equal(actual, expected)?;
            model.outcome(op, expected);
            if let Err(e) = expected {
                model.fail(e);
            }
        }
        Op::Set(id, child) => {
            let expected = model
                .valid(id)
                .and_then(|_| {
                    if !model.nodes[id].object {
                        Err(M::NotObject)
                    } else {
                        Ok(())
                    }
                })
                .and_then(|_| {
                    child.map_or(Ok(()), |c| {
                        model.valid(c).and_then(|_| {
                            if model.nodes[c].region != model.nodes[id].region {
                                Err(M::CrossRegionEdge)
                            } else {
                                Ok(())
                            }
                        })
                    })
                });
            equal(
                session.set_reference(handles[id], 0, child.map(|c| handles[c])),
                expected,
            )?;
            model.outcome(op, expected);
            match expected {
                Ok(()) => model.nodes[id].child = child,
                Err(error) => model.fail(error),
            }
        }
        Op::Link(id, child, fault) => {
            let r = model.nodes[id].region;
            let mut expected = model
                .valid(id)
                .and_then(|_| model.valid(child))
                .and_then(|_| {
                    if r != model.nodes[child].region {
                        Err(M::CrossRegionEdge)
                    } else {
                        Ok(())
                    }
                });
            let fresh = !model.nodes[id].edges.contains(&child);
            if expected.is_ok() && fresh {
                if EDGE_METADATA_BYTES > LIMIT - model.stats[r].committed_bytes {
                    expected = Err(M::Allocation(A::RegionLimit));
                } else if fault {
                    expected = Err(M::Allocation(A::HostAllocation));
                }
                if expected.is_err() {
                    model.stats[r].allocation_failures += 1;
                } else {
                    model.nodes[id].edges.insert(child);
                    model.nodes[id].charge += EDGE_METADATA_BYTES;
                    model.charge(r, EDGE_METADATA_BYTES, EDGE_METADATA_BYTES);
                }
            }
            #[cfg(feature = "fault-injection")]
            if fault {
                session.inject_host_failure_at_reserve(1);
            }
            equal(session.link(handles[id], handles[child]), expected)?;
            model.outcome(op, expected);
            #[cfg(feature = "fault-injection")]
            if fault {
                equal(
                    session.last_host_failure_site(),
                    (expected == Err(M::Allocation(A::HostAllocation)))
                        .then_some(ReserveSite::Edges),
                )?;
                session.disable_host_failure();
            }
            if let Err(e) = expected {
                model.fail(e);
            }
        }
        Op::Copy(id, r, fault) => {
            let mut expected = model.valid(id);
            if expected.is_ok() && fault {
                expected = Err(M::Allocation(A::HostAllocation));
            }
            #[cfg(feature = "fault-injection")]
            if fault {
                session.inject_host_failure_at_reserve(1);
            }
            let actual = session.copy_managed_to_owned(handles[id]);
            equal(actual.as_ref().map(|_| ()).map_err(|e| *e), expected)?;
            model.outcome(op, expected);
            #[cfg(feature = "fault-injection")]
            if fault {
                equal(
                    session.last_host_failure_site(),
                    (expected == Err(M::Allocation(A::HostAllocation)))
                        .then_some(ReserveSite::ManagedToOwned),
                )?;
                session.disable_host_failure();
            }
            if let Ok(mut owned) = actual {
                // Use expected source bytes, never runtime bytes, for the copied oracle value.
                let mut bytes = model.nodes[id].bytes.clone();
                bytes[0] ^= 0x5a;
                owned.as_mut_slice()[0] ^= 0x5a;
                let expected = model.alloc(r, false, bytes.clone(), Fault::None, true);
                let actual = match session.promote_owned(regions[r], owned) {
                    Ok(handle) => Ok(handle),
                    Err((error, owned)) => {
                        equal(owned.as_slice(), bytes.as_slice())?;
                        Err(error)
                    }
                };
                allocation_result(model, handles, actual, expected)?;
                model.outcome(op, expected.map(|_| ()));
            } else {
                model.fail(expected.unwrap_err());
            }
        }
        Op::Collect(r) => {
            let expected = if !model.budgets.is_empty() {
                Err(M::CollectionForbidden)
            } else if model.closed[r] {
                Err(M::InvalidRegion)
            } else {
                Ok(())
            };
            equal(session.collect(regions[r]), expected)?;
            model.outcome(op, expected);
            match expected {
                Ok(()) => model.collect(r, false),
                Err(error) => model.fail(error),
            }
        }
        Op::Close(r) => {
            let expected = if model.closed[r] {
                Err(M::InvalidRegion)
            } else if model.census(Some(r)).rooted_handles > 0 {
                Err(M::RegionHasRoots)
            } else {
                Ok(())
            };
            equal(session.close_region(regions[r]), expected)?;
            model.outcome(op, expected);
            if let Err(error) = expected {
                model.fail(error);
            } else {
                model.closed[r] = true;
                model.stats[r].committed_bytes = 0;
                model.stats[r].live_bytes = 0;
                for n in &mut model.nodes {
                    if n.region == r {
                        n.live = false;
                    }
                }
            }
        }
        Op::Nested(first) => {
            // Both regions participate in both guards; exit itself must never collect.
            let mut outer = session.no_gc(3 * C);
            model.budgets.push(3 * C);
            {
                let mut inner = outer.no_gc(C);
                model.budgets.push(C);
                for nested in [
                    Op::Alloc(first, true, 1, Fault::None),
                    Op::Alloc(1 - first, true, 1, Fault::None),
                    Op::Collect(0),
                    Op::Collect(1),
                ] {
                    apply(nested, &mut inner, regions, model, handles)?;
                }
                model.budgets.pop();
            }
            apply(
                Op::Alloc(1 - first, true, 1, Fault::None),
                &mut outer,
                regions,
                model,
                handles,
            )?;
            model.budgets.pop();
            drop(outer);
            model.outcome(op, Ok::<(), M>(()));
        }
    }
    model.check(session, regions, handles)
}
struct Runner {
    session: Session,
    regions: [RegionId; 2],
    model: Model,
    handles: Vec<Handle>,
    trace: Vec<Op>,
    previous: &'static str,
}
impl Runner {
    fn replay_step(&mut self, seed: u64, steps: usize, op: Op) {
        if let Err(error) = self.step(op) {
            let prefix = self.trace.len();
            let source = self
                .trace
                .iter()
                .map(|op| op.rust())
                .collect::<Vec<_>>()
                .join(",\n");
            let feature = if cfg!(feature = "fault-injection") {
                "--features fault-injection"
            } else {
                ""
            };
            panic!("seed={seed} steps={steps} first divergent prefix={prefix} first modeled failure={:?}\n{error}\nExecutable operations (replay with Runner::step):\nvec![\n{source}\n]\nCGX2_W23_SEED={seed} CGX2_W23_COUNT=1 CGX2_W23_STEPS={steps} CGX2_W23_STOP={prefix} cargo +1.98.1 test -p cgx2-runtime {feature} --test automatic_pressure_wave23 pressure_corpus --offline -- --nocapture",self.model.first_failure);
        }
    }
    fn new(seed: u64) -> Self {
        let model = Model::new(seed);
        let mut session = Session::new();
        let regions = std::array::from_fn(|r| {
            session
                .create_region(
                    if r == model.auto_region {
                        CollectionMode::Auto {
                            trigger_bytes: model.trigger,
                        }
                    } else {
                        CollectionMode::Manual
                    },
                    LIMIT,
                )
                .unwrap()
        });
        Self {
            session,
            regions,
            model,
            handles: vec![],
            trace: vec![],
            previous: "start",
        }
    }
    fn step(&mut self, op: Op) -> Result<(), String> {
        self.trace.push(op);
        *self
            .model
            .coverage
            .transitions
            .entry(format!("{}->{}", self.previous, op.name()))
            .or_default() += 1;
        self.previous = op.name();
        apply(
            op,
            &mut self.session,
            self.regions,
            &mut self.model,
            &mut self.handles,
        )
    }
}
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
fn generate(run: &Runner, state: &mut u64, close_after: usize) -> Op {
    let x = next(state);
    let r = ((x >> 8) & 1) as usize;
    let live: Vec<_> = run
        .model
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, n)| n.live.then_some(id))
        .collect();
    let pick = |bits: u64| {
        if x & 64 != 0 && !live.is_empty() {
            live[bits as usize % live.len()]
        } else {
            bits as usize % run.model.nodes.len()
        }
    };
    let id = pick(x >> 16);
    let child = pick(x >> 32);
    let fault = cfg!(feature = "fault-injection") && (x >> 48).is_multiple_of(7);
    match x % 13 {
        0..=3 => {
            let object = x & 16 != 0;
            Op::Alloc(
                r,
                object,
                1 + (x as usize % 7),
                if fault {
                    if object && x & 32 != 0 {
                        Fault::References
                    } else {
                        Fault::Payload
                    }
                } else {
                    Fault::None
                },
            )
        }
        4 => Op::Root(id),
        5 => Op::Unroot(id),
        6 => Op::Set(id, if x & 16 != 0 { Some(child) } else { None }),
        7 => Op::Link(id, child, fault),
        8 => Op::Copy(id, r, fault),
        9 => Op::Collect(r),
        10 => Op::Nested(r),
        11 if run.trace.len() > close_after => Op::Close(r),
        _ => Op::Unroot(child),
    }
}
fn replay(seed: u64, steps: usize, stop: usize) -> Runner {
    let mut run = Runner::new(seed);
    let a = run.model.auto_region;
    // A guaranteed pressure graph plus varied generated tail: roots, cycles, deferred
    // garbage, duplicate roots, slot reuse, nested budgets, and retry after exit.
    let prologue = [
        Op::Alloc(a, true, 1, Fault::None),
        Op::Root(0),
        Op::Root(0),
        Op::Alloc(a, true, 1, Fault::None),
        Op::Set(0, Some(1)),
        Op::Link(1, 0, false),
        Op::Alloc(1 - a, true, 1, Fault::None),
        Op::Root(2),
        Op::Nested(a),
        Op::Alloc(a, false, 3, Fault::None),
        Op::Unroot(0),
        Op::Unroot(0),
        Op::Set(0, None),
        Op::Alloc(a, true, 1, Fault::None),
        Op::Alloc(a, false, 1, Fault::None),
        Op::Copy(2, a, false),
        Op::Collect(1 - a),
    ];
    let mut state = seed;
    for index in 0..prologue.len() + steps {
        if index == stop {
            return run;
        }
        let op = if index < prologue.len() {
            prologue[index]
        } else {
            generate(&run, &mut state, prologue.len() + 3 * steps / 4)
        };
        run.replay_step(seed, steps, op);
    }
    // Deterministic cleanup, region order seed-selected. Closed/invalid cases remain checked.
    for id in 0..run.model.nodes.len() {
        while run.model.nodes[id].live && run.model.nodes[id].roots > 0 {
            if run.trace.len() == stop {
                return run;
            }
            run.replay_step(seed, steps, Op::Unroot(id));
        }
    }
    for r in [a, 1 - a] {
        if run.trace.len() == stop {
            return run;
        }
        run.replay_step(seed, steps, Op::Close(r));
    }
    run
}
fn setting(name: &str, default: u64, max: u64) -> u64 {
    let value = std::env::var(name)
        .map(|v| v.parse().expect("expected decimal u64"))
        .unwrap_or(default);
    assert!(value <= max, "{name} exceeds bound {max}");
    value
}
#[test]
fn pressure_corpus() {
    let steps = setting("CGX2_W23_STEPS", 128, 4096) as usize;
    let stop = setting("CGX2_W23_STOP", u64::MAX, u64::MAX) as usize;
    let seeds: Vec<u64> = if std::env::var_os("CGX2_W23_SEED").is_some() {
        let start = setting("CGX2_W23_SEED", 0, u64::MAX);
        let count = setting("CGX2_W23_COUNT", 1, 10000);
        assert!(count > 0);
        (0..count).map(|i| start.wrapping_add(i)).collect()
    } else {
        SEEDS
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| line.parse().unwrap())
            .collect()
    };
    let mut coverage = Coverage::default();
    for &seed in &seeds {
        let run = replay(seed, steps, stop);
        if std::env::var_os("CGX2_W23_TRACE").is_some() {
            eprintln!("seed={seed} steps={steps} trace=vec![");
            for op in &run.trace {
                eprintln!("    {},", op.rust());
            }
            eprintln!("]");
        }
        coverage.merge(&run.model.coverage);
    }
    eprintln!(
        "Wave23 seeds={} steps={steps} fault={} coverage={coverage:?}",
        seeds.len(),
        cfg!(feature = "fault-injection")
    );
    if stop == usize::MAX {
        assert!(
            coverage.auto > 0
                && coverage.rooted_survivors > 0
                && coverage.reclaimed > 0
                && coverage.reused > 0
                && coverage.budget_failure > 0
        );
    }
}
#[test]
fn corrupted_oracle_is_rejected() {
    let mut run = Runner::new(0);
    run.step(Op::Alloc(0, true, 1, Fault::None)).unwrap();
    run.model.stats[0].committed_bytes += 1;
    assert!(run
        .model
        .check(&run.session, run.regions, &run.handles)
        .is_err());
}
#[cfg(feature = "fault-injection")]
#[test]
fn reserve_failures_before_and_after_collection() {
    // Payload/refs fail before collection even at threshold. ManagedBytes fails
    // after collection and can reclaim a whole unrooted graph before returning Err.
    for (object, fault, index, site) in [
        (true, Fault::Payload, 1, ReserveSite::ObjectPayload),
        (true, Fault::References, 2, ReserveSite::ObjectReferences),
        (false, Fault::Payload, 1, ReserveSite::ManagedBytes),
    ] {
        let mut run = Runner::new(0);
        let a = run.model.auto_region;
        run.model.trigger = 2 * C; // seed 0 already has this runtime threshold.
        for op in [
            Op::Alloc(a, true, 1, Fault::None),
            Op::Root(0),
            Op::Alloc(a, true, 1, Fault::None),
            Op::Set(0, Some(1)),
            Op::Alloc(1 - a, true, 1, Fault::None),
            Op::Root(2),
        ] {
            run.step(op).unwrap();
        }
        // A reachable graph survives the first failed allocation.
        let before = run.model.stats[a];
        run.step(Op::Alloc(a, object, 1, fault)).unwrap();
        equal(run.session.host_reserve_attempts(), index).unwrap();
        equal(run.session.last_host_failure_site(), Some(site)).unwrap();
        equal(
            run.model.stats[a].collection_count,
            before.collection_count + u64::from(!object),
        )
        .unwrap();
        equal(run.model.stats[a].committed_bytes, before.committed_bytes).unwrap();
        run.step(Op::Unroot(0)).unwrap();
        run.step(Op::Alloc(a, object, 1, fault)).unwrap();
        equal(
            run.model.stats[a].total_collected_bytes,
            if object { 0 } else { 2 * C },
        )
        .unwrap();
        equal(run.model.first_failure.as_deref(), Some("HostAllocation")).unwrap();
        run.step(Op::Alloc(a, object, 1, Fault::None)).unwrap(); // one-shot failure, recover
        equal(run.model.first_failure.as_deref(), Some("HostAllocation")).unwrap();
    }
    // A fresh table has zero capacity. trigger=0 guarantees a collection before
    // SlotTable reserve; a failed slot reserve commits no object or generation.
    for object in [false, true] {
        let mut run = Runner::new(0);
        run.session = Session::new();
        run.model.trigger = 0;
        run.regions = [
            run.session
                .create_region(CollectionMode::Auto { trigger_bytes: 0 }, LIMIT)
                .unwrap(),
            run.session
                .create_region(CollectionMode::Manual, LIMIT)
                .unwrap(),
        ];
        run.step(Op::Alloc(0, object, 1, Fault::Slot)).unwrap();
        equal(
            run.session.last_host_failure_site(),
            Some(ReserveSite::SlotTable),
        )
        .unwrap();
        equal(run.model.stats[0].collection_count, 1).unwrap();
        run.step(Op::Alloc(0, object, 1, Fault::None)).unwrap();
        equal(run.handles[0].generation(), 0).unwrap();
    }
}

#[test]
fn both_nested_budget_orders_and_exit_retry() {
    for a in [0, 1] {
        for (outer_budget, inner_budget) in [(C, 3 * C), (3 * C, C)] {
            let mut run = Runner::new(3 * a as u64);
            // Put both regions under pressure before entering either guard.
            for r in [0, 1] {
                for _ in 0..4 {
                    run.step(Op::Alloc(r, true, 1, Fault::None)).unwrap();
                }
            }
            let before = run.model.stats.map(|s| s.collection_count);
            let mut outer = run.session.no_gc(outer_budget);
            run.model.budgets.push(outer_budget);
            {
                let mut inner = outer.no_gc(inner_budget);
                run.model.budgets.push(inner_budget);
                apply(
                    Op::Alloc(a, true, 1, Fault::None),
                    &mut inner,
                    run.regions,
                    &mut run.model,
                    &mut run.handles,
                )
                .unwrap();
                #[cfg(feature = "fault-injection")]
                inner.inject_host_failure_at_reserve(1);
                apply(
                    Op::Alloc(1 - a, true, 1, Fault::None),
                    &mut inner,
                    run.regions,
                    &mut run.model,
                    &mut run.handles,
                )
                .unwrap();
                #[cfg(feature = "fault-injection")]
                {
                    equal(inner.host_reserve_attempts(), 0).unwrap();
                    equal(inner.last_host_failure_site(), None).unwrap();
                    inner.disable_host_failure();
                }
                equal(run.model.first_failure.as_deref(), Some("BudgetExceeded")).unwrap();
                for r in [0, 1] {
                    apply(
                        Op::Collect(r),
                        &mut inner,
                        run.regions,
                        &mut run.model,
                        &mut run.handles,
                    )
                    .unwrap();
                }
                run.model.budgets.pop();
            }
            run.model.budgets.pop();
            drop(outer);
            run.model
                .check(&run.session, run.regions, &run.handles)
                .unwrap();
            equal(run.model.stats.map(|s| s.collection_count), before).unwrap();
            run.step(Op::Alloc(a, true, 1, Fault::None)).unwrap();
            run.step(Op::Alloc(1 - a, true, 1, Fault::None)).unwrap();
            equal(run.model.stats[a].collection_count, before[a] + 1).unwrap();
            equal(run.model.stats[1 - a].collection_count, before[1 - a]).unwrap();
            // Later failures cannot replace the first diagnostic.
            run.step(Op::Root(0)).unwrap();
            equal(run.model.first_failure.as_deref(), Some("BudgetExceeded")).unwrap();
        }
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn non_safepoint_reserve_sites_preserve_graph_and_first_failure() {
    let mut run = Runner::new(0);
    for op in [
        Op::Alloc(0, true, 1, Fault::None),
        Op::Root(0),
        Op::Alloc(0, true, 1, Fault::None),
        Op::Set(0, Some(1)),
        Op::Alloc(1, true, 1, Fault::None),
        Op::Root(2),
    ] {
        run.step(op).unwrap();
    }
    let collections = run.model.stats.map(|s| s.collection_count);
    run.step(Op::Link(0, 1, true)).unwrap();
    equal(
        run.session.last_host_failure_site(),
        Some(ReserveSite::Edges),
    )
    .unwrap();
    let first = run.model.first_failure.clone();
    run.step(Op::Copy(0, 1, true)).unwrap();
    equal(
        run.session.last_host_failure_site(),
        Some(ReserveSite::ManagedToOwned),
    )
    .unwrap();
    equal(run.model.first_failure.clone(), first.clone()).unwrap();
    // These session-wide reserves have no region allocation-failure charge and
    // cannot collect either region, including when auto is at its threshold.
    run.session.inject_host_failure_at_reserve(1);
    let error = run.session.alloc_owned_bytes(1).map(|_| ()).unwrap_err();
    equal(error, A::HostAllocation).unwrap();
    equal(
        run.session.last_host_failure_site(),
        Some(ReserveSite::OwnedBytes),
    )
    .unwrap();
    run.model.fail(error);
    run.model
        .check(&run.session, run.regions, &run.handles)
        .unwrap();
    equal(run.model.first_failure.clone(), first.clone()).unwrap();
    equal(run.model.stats.map(|s| s.collection_count), collections).unwrap();
    run.step(Op::Link(0, 1, false)).unwrap();
    run.step(Op::Copy(0, 1, false)).unwrap();
    run.step(Op::Alloc(0, true, 1, Fault::None)).unwrap();
    // Session::new starts with the one default region in a one-element Vec.
    // This targets actual region-table growth without guessing spare capacity
    // after the two-region model fixture has been created.
    let mut session = Session::new();
    let handle = session
        .alloc_object(session.default_region(), 1, 1)
        .unwrap();
    session.root(handle).unwrap();
    let before = session.memory_report();
    session.inject_host_failure_at_reserve(1);
    equal(
        session.create_region(CollectionMode::Manual, LIMIT),
        Err(A::HostAllocation),
    )
    .unwrap();
    equal(
        session.last_host_failure_site(),
        Some(ReserveSite::RegionTable),
    )
    .unwrap();
    equal(session.memory_report(), before).unwrap();
    equal(session.read_bytes(handle), Ok(&[0][..])).unwrap();
}

#[test]
fn rooted_region_limit_failure_can_follow_collection() {
    let mut run = Runner::new(0);
    for id in 0..12 {
        run.step(Op::Alloc(0, true, 1, Fault::None)).unwrap();
        run.step(Op::Root(id)).unwrap();
    }
    let before = run.model.stats[0];
    run.step(Op::Alloc(0, false, 1, Fault::None)).unwrap();
    equal(
        run.model.stats[0].collection_count,
        before.collection_count + 1,
    )
    .unwrap();
    equal(run.model.stats[0].committed_bytes, LIMIT).unwrap();
    equal(run.model.stats[0].allocation_failures, 1).unwrap();
    equal(run.model.stats[0].total_collected_bytes, 0).unwrap();
    equal(run.model.first_failure.as_deref(), Some("RegionLimit")).unwrap();
}
