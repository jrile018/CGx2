//! Independent byte-object transition model for Wave 10 allocation ordering.
//! V4's generator and serialized audit remain frozen.
use std::collections::BTreeMap;

use cgx2_runtime::{
    AllocationError as A, CollectionMode, Handle, MemoryError as M, OwnedBytes, RegionId,
    RegionStats, Session, EDGE_METADATA_BYTES, OWNED_METADATA_BYTES, REGION_METADATA_BYTES,
    SLOT_METADATA_BYTES,
};

pub const MODEL_VERSION_V5: u32 = 5;
const MAX_STEPS: usize = 96;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Create {
        manual: bool,
        limit: usize,
        trigger: usize,
    },
    Alloc {
        region: usize,
        len: usize,
    },
    Owned {
        len: usize,
    },
    MutateOwned {
        owner: usize,
        byte: u8,
    },
    Promote {
        region: usize,
        owner: usize,
    },
    Copy {
        handle: usize,
    },
    Root {
        handle: usize,
    },
    Unroot {
        handle: usize,
    },
    Link {
        parent: usize,
        child: usize,
    },
    Read {
        handle: usize,
    },
    Collect {
        region: usize,
    },
    Close {
        region: usize,
    },
    GuardStart {
        budget: usize,
    },
    GuardEnd,
}

#[derive(Clone)]
struct Object {
    region: usize,
    bytes: Vec<u8>,
    roots: usize,
    edges: Vec<usize>,
    charge: usize,
    valid: bool,
}

#[derive(Clone)]
struct Region {
    open: bool,
    mode: CollectionMode,
    limit: usize,
    stats: RegionStats,
}

struct Model {
    regions: Vec<Region>,
    objects: Vec<Object>,
    owned: Vec<Option<Vec<u8>>>,
    budgets: Vec<(usize, usize)>,
    fault: bool,
    fault_applied: bool,
}

impl Model {
    fn new(fault: bool) -> Self {
        Self {
            regions: vec![Region {
                open: true,
                mode: CollectionMode::Auto {
                    trigger_bytes: 1 << 20,
                },
                limit: 64 << 20,
                stats: RegionStats::default(),
            }],
            objects: Vec::new(),
            owned: Vec::new(),
            budgets: Vec::new(),
            fault,
            fault_applied: false,
        }
    }

    fn budget(&self, charge: usize) -> Result<(), A> {
        if self
            .budgets
            .iter()
            .any(|(limit, spent)| charge > limit.saturating_sub(*spent))
        {
            Err(A::BudgetExceeded)
        } else {
            Ok(())
        }
    }

    fn charge(&mut self, charge: usize) {
        for (_, spent) in &mut self.budgets {
            *spent += charge;
        }
    }

    fn valid(&self, handle: usize) -> Result<&Object, M> {
        self.objects
            .get(handle)
            .filter(|o| o.valid && self.regions[o.region].open)
            .ok_or(M::InvalidHandle)
    }

    fn region(&self, region: usize) -> Result<&Region, M> {
        self.regions
            .get(region)
            .filter(|r| r.open)
            .ok_or(M::InvalidRegion)
    }

    fn collect(&mut self, region: usize) {
        let mut marked = vec![false; self.objects.len()];
        for (i, object) in self.objects.iter().enumerate() {
            if object.valid && object.region == region && object.roots > 0 {
                marked[i] = true;
            }
        }
        loop {
            let mut changed = false;
            for i in 0..self.objects.len() {
                if !marked[i] {
                    continue;
                }
                for &child in &self.objects[i].edges {
                    if self.objects[child].valid && !marked[child] {
                        marked[child] = true;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut reclaimed = 0;
        for (i, object) in self.objects.iter_mut().enumerate() {
            if object.valid && object.region == region && !marked[i] {
                object.valid = false;
                reclaimed += object.charge;
            }
        }
        let stats = &mut self.regions[region].stats;
        stats.committed_bytes -= reclaimed;
        stats.live_bytes -= reclaimed;
        stats.total_collected_bytes += reclaimed;
        stats.collection_count += 1;
    }

    fn alloc(&mut self, region: usize, len: usize) -> Result<(), A> {
        let r = self.region(region).map_err(|_| A::InvalidRegion)?;
        let requested = match len.checked_add(SLOT_METADATA_BYTES) {
            Some(value) => value,
            None => return self.failed(region, A::HostAllocation),
        };
        if requested > r.limit {
            return self.failed(region, A::RegionLimit);
        }
        if len > isize::MAX as usize {
            return self.failed(region, A::HostAllocation);
        }
        if let Err(error) = self.budget(requested) {
            return self.failed(region, error);
        }
        let should_collect = self.budgets.is_empty()
            && matches!(r.mode, CollectionMode::Auto { trigger_bytes }
                if r.stats.committed_bytes >= trigger_bytes
                    || requested > r.limit.saturating_sub(r.stats.committed_bytes));
        if should_collect {
            self.collect(region);
        }
        if requested
            > self.regions[region]
                .limit
                .saturating_sub(self.regions[region].stats.committed_bytes)
        {
            return self.failed(region, A::RegionLimit);
        }
        let stats = &mut self.regions[region].stats;
        stats.committed_bytes += requested;
        stats.live_bytes += requested;
        stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
        self.charge(requested);
        self.objects.push(Object {
            region,
            bytes: vec![0; len],
            roots: 0,
            edges: Vec::new(),
            charge: requested,
            valid: true,
        });
        if self.fault && !self.fault_applied {
            self.regions[region].stats.committed_bytes += 1;
            self.fault_applied = true;
        }
        Ok(())
    }

    fn failed(&mut self, region: usize, error: A) -> Result<(), A> {
        self.regions[region].stats.allocation_failures += 1;
        Err(error)
    }

    fn apply(&mut self, op: &Op) -> String {
        match *op {
            Op::Create {
                manual,
                limit,
                trigger,
            } => {
                if let Err(error) = self.budget(REGION_METADATA_BYTES) {
                    return outcome::<(), _>(Err(error));
                }
                self.regions.push(Region {
                    open: true,
                    mode: if manual {
                        CollectionMode::Manual
                    } else {
                        CollectionMode::Auto {
                            trigger_bytes: trigger,
                        }
                    },
                    limit,
                    stats: RegionStats::default(),
                });
                self.charge(REGION_METADATA_BYTES);
                "Ok".into()
            }
            Op::Alloc { region, len } => outcome(self.alloc(region, len)),
            Op::Owned { len } => {
                let charge = match len.checked_add(OWNED_METADATA_BYTES) {
                    Some(c) => c,
                    None => return outcome::<(), _>(Err(A::HostAllocation)),
                };
                if let Err(error) = self.budget(charge) {
                    return outcome::<(), _>(Err(error));
                }
                if len > isize::MAX as usize {
                    return outcome::<(), _>(Err(A::HostAllocation));
                }
                self.charge(charge);
                self.owned.push(Some(vec![0; len]));
                "Ok".into()
            }
            Op::MutateOwned { owner, byte } => {
                match self.owned.get_mut(owner).and_then(Option::as_mut) {
                    Some(bytes) => {
                        if let Some(first) = bytes.first_mut() {
                            *first = byte;
                        }
                        "Ok".into()
                    }
                    None => "Skipped".into(),
                }
            }
            Op::Promote { region, owner } => {
                let Some(bytes) = self.owned.get(owner).and_then(Option::as_ref) else {
                    return "Skipped".into();
                };
                let len = bytes.len();
                if self.region(region).is_err() {
                    return outcome::<(), _>(Err(A::InvalidRegion));
                }
                let charge = SLOT_METADATA_BYTES + len;
                if let Err(error) = self.budget(SLOT_METADATA_BYTES) {
                    return self.failed_string(region, error);
                }
                if charge
                    > self.regions[region]
                        .limit
                        .saturating_sub(self.regions[region].stats.committed_bytes)
                {
                    return self.failed_string(region, A::RegionLimit);
                }
                let bytes = self.owned[owner].take().expect("checked owner");
                let stats = &mut self.regions[region].stats;
                stats.committed_bytes += charge;
                stats.live_bytes += charge;
                stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
                self.charge(SLOT_METADATA_BYTES);
                self.objects.push(Object {
                    region,
                    bytes,
                    roots: 0,
                    edges: Vec::new(),
                    charge,
                    valid: true,
                });
                "Ok".into()
            }
            Op::Copy { handle } => {
                let bytes = match self.valid(handle) {
                    Ok(o) => o.bytes.clone(),
                    Err(error) => return outcome::<(), _>(Err(error)),
                };
                let charge = OWNED_METADATA_BYTES + bytes.len();
                if let Err(error) = self.budget(charge) {
                    return outcome::<(), _>(Err(M::Allocation(error)));
                }
                self.charge(charge);
                self.owned.push(Some(bytes));
                "Ok".into()
            }
            Op::Root { handle } => match self.valid(handle) {
                Ok(_) => {
                    self.objects[handle].roots += 1;
                    "Ok".into()
                }
                Err(e) => outcome::<(), _>(Err(e)),
            },
            Op::Unroot { handle } => match self.valid(handle) {
                Err(e) => outcome::<(), _>(Err(e)),
                Ok(o) if o.roots == 0 => outcome::<(), _>(Err(M::NotRooted)),
                Ok(_) => {
                    self.objects[handle].roots -= 1;
                    "Ok".into()
                }
            },
            Op::Link { parent, child } => {
                let (p, c) = match (self.valid(parent), self.valid(child)) {
                    (Ok(p), Ok(c)) => (p.region, c.region),
                    _ => return outcome::<(), _>(Err(M::InvalidHandle)),
                };
                if p != c {
                    return outcome::<(), _>(Err(M::CrossRegionEdge));
                }
                if self.objects[parent].edges.contains(&child) {
                    return "Ok".into();
                }
                if let Err(e) = self.budget(EDGE_METADATA_BYTES) {
                    self.regions[p].stats.allocation_failures += 1;
                    return outcome::<(), _>(Err(M::Allocation(e)));
                }
                if EDGE_METADATA_BYTES
                    > self.regions[p]
                        .limit
                        .saturating_sub(self.regions[p].stats.committed_bytes)
                {
                    self.regions[p].stats.allocation_failures += 1;
                    return outcome::<(), _>(Err(M::Allocation(A::RegionLimit)));
                }
                self.objects[parent].edges.push(child);
                self.objects[parent].charge += EDGE_METADATA_BYTES;
                let stats = &mut self.regions[p].stats;
                stats.committed_bytes += EDGE_METADATA_BYTES;
                stats.live_bytes += EDGE_METADATA_BYTES;
                stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
                self.charge(EDGE_METADATA_BYTES);
                "Ok".into()
            }
            Op::Read { handle } => match self.valid(handle) {
                Ok(object) => format!("Ok({:?})", object.bytes),
                Err(e) => outcome::<(), _>(Err(e)),
            },
            Op::Collect { region } => {
                if !self.budgets.is_empty() {
                    return outcome::<(), _>(Err(M::CollectionForbidden));
                }
                if self.region(region).is_err() {
                    return outcome::<(), _>(Err(M::InvalidRegion));
                }
                self.collect(region);
                "Ok".into()
            }
            Op::Close { region } => {
                if self.region(region).is_err() {
                    return outcome::<(), _>(Err(M::InvalidRegion));
                }
                if self
                    .objects
                    .iter()
                    .any(|o| o.valid && o.region == region && o.roots > 0)
                {
                    return outcome::<(), _>(Err(M::RegionHasRoots));
                }
                for o in &mut self.objects {
                    if o.region == region {
                        o.valid = false;
                    }
                }
                let r = &mut self.regions[region];
                r.open = false;
                r.stats.committed_bytes = 0;
                r.stats.live_bytes = 0;
                "Ok".into()
            }
            Op::GuardStart { budget } => {
                self.budgets.push((budget, 0));
                "Ok".into()
            }
            Op::GuardEnd => {
                self.budgets.pop();
                "Ok".into()
            }
        }
    }

    fn failed_string(&mut self, region: usize, error: A) -> String {
        outcome(self.failed(region, error))
    }
}

fn outcome<T: std::fmt::Debug, E: std::fmt::Debug>(result: Result<T, E>) -> String {
    match result {
        Ok(_) => "Ok".into(),
        Err(e) => format!("Err({e:?})"),
    }
}

struct Actual {
    regions: Vec<RegionId>,
    objects: Vec<Handle>,
    owned: Vec<Option<OwnedBytes>>,
}

impl Actual {
    fn new(session: &Session) -> Self {
        Self {
            regions: vec![session.default_region()],
            objects: Vec::new(),
            owned: Vec::new(),
        }
    }

    fn apply(&mut self, session: &mut Session, op: &Op) -> String {
        match *op {
            Op::Create {
                manual,
                limit,
                trigger,
            } => {
                let mode = if manual {
                    CollectionMode::Manual
                } else {
                    CollectionMode::Auto {
                        trigger_bytes: trigger,
                    }
                };
                match session.create_region(mode, limit) {
                    Ok(r) => {
                        self.regions.push(r);
                        "Ok".into()
                    }
                    Err(e) => outcome::<(), _>(Err(e)),
                }
            }
            Op::Alloc { region, len } => match self.regions.get(region) {
                Some(&r) => match session.alloc_bytes(r, len) {
                    Ok(h) => {
                        self.objects.push(h);
                        "Ok".into()
                    }
                    Err(e) => outcome::<(), _>(Err(e)),
                },
                None => "Skipped".into(),
            },
            Op::Owned { len } => match session.alloc_owned_bytes(len) {
                Ok(value) => {
                    self.owned.push(Some(value));
                    "Ok".into()
                }
                Err(e) => outcome::<(), _>(Err(e)),
            },
            Op::MutateOwned { owner, byte } => {
                match self.owned.get_mut(owner).and_then(Option::as_mut) {
                    Some(value) => {
                        if let Some(first) = value.as_mut_slice().first_mut() {
                            *first = byte;
                        }
                        "Ok".into()
                    }
                    None => "Skipped".into(),
                }
            }
            Op::Promote { region, owner } => {
                let (Some(&r), Some(slot)) = (self.regions.get(region), self.owned.get_mut(owner))
                else {
                    return "Skipped".into();
                };
                let Some(value) = slot.take() else {
                    return "Skipped".into();
                };
                match session.promote_owned(r, value) {
                    Ok(h) => {
                        self.objects.push(h);
                        "Ok".into()
                    }
                    Err((e, value)) => {
                        *slot = Some(value);
                        outcome::<(), _>(Err(e))
                    }
                }
            }
            Op::Copy { handle } => {
                let Some(&h) = self.objects.get(handle) else {
                    return "Skipped".into();
                };
                match session.copy_managed_to_owned(h) {
                    Ok(value) => {
                        self.owned.push(Some(value));
                        "Ok".into()
                    }
                    Err(e) => outcome::<(), _>(Err(e)),
                }
            }
            Op::Root { handle } => self
                .objects
                .get(handle)
                .map_or_else(|| "Skipped".into(), |&h| outcome(session.root(h))),
            Op::Unroot { handle } => self
                .objects
                .get(handle)
                .map_or_else(|| "Skipped".into(), |&h| outcome(session.unroot(h))),
            Op::Link { parent, child } => match (self.objects.get(parent), self.objects.get(child))
            {
                (Some(&p), Some(&c)) => outcome(session.link(p, c)),
                _ => "Skipped".into(),
            },
            Op::Read { handle } => self.objects.get(handle).map_or_else(
                || "Skipped".into(),
                |&h| match session.read_bytes(h) {
                    Ok(b) => format!("Ok({b:?})"),
                    Err(e) => outcome::<(), _>(Err(e)),
                },
            ),
            Op::Collect { region } => self
                .regions
                .get(region)
                .map_or_else(|| "Skipped".into(), |&r| outcome(session.collect(r))),
            Op::Close { region } => self
                .regions
                .get(region)
                .map_or_else(|| "Skipped".into(), |&r| outcome(session.close_region(r))),
            Op::GuardStart { .. } | Op::GuardEnd => "Ok".into(),
        }
    }
}

fn compare(session: &Session, actual: &Actual, model: &Model) -> Result<(), String> {
    let report = session.memory_report();
    if report.regions.len() != model.regions.len() {
        return Err("region count".into());
    }
    let mut totals = RegionStats::default();
    for (i, (r, expected)) in report.regions.iter().zip(&model.regions).enumerate() {
        let stats = RegionStats {
            committed_bytes: r.committed_bytes,
            live_bytes: r.live_bytes,
            peak_bytes: r.peak_bytes,
            allocation_failures: r.allocation_failures,
            collection_count: r.collection_count,
            total_collected_bytes: r.total_collected_bytes,
        };
        if r.is_open != expected.open
            || r.mode != expected.mode
            || r.limit_bytes != expected.limit
            || stats != expected.stats
        {
            return Err(format!(
                "region {i}: expected {:?}, got {stats:?}",
                expected.stats
            ));
        }
        if r.pause.sample_count != stats.collection_count
            || stats.live_bytes > stats.committed_bytes
            || (r.is_open && stats.committed_bytes > r.limit_bytes)
        {
            return Err(format!("region {i}: accounting invariant"));
        }
        totals.committed_bytes += stats.committed_bytes;
        totals.live_bytes += stats.live_bytes;
        totals.peak_bytes += stats.peak_bytes;
        totals.allocation_failures += stats.allocation_failures;
        totals.collection_count += stats.collection_count;
        totals.total_collected_bytes += stats.total_collected_bytes;
    }
    if report.totals != totals {
        return Err(format!(
            "totals: expected {totals:?}, got {:?}",
            report.totals
        ));
    }
    if actual.objects.len() != model.objects.len() || actual.owned.len() != model.owned.len() {
        return Err("retained value count".into());
    }
    for (i, object) in model.objects.iter().enumerate() {
        let got = session.read_bytes(actual.objects[i]);
        if object.valid && model.regions[object.region].open {
            if got != Ok(object.bytes.as_slice()) {
                return Err(format!(
                    "handle {i}: expected {:?}, got {got:?}",
                    object.bytes
                ));
            }
        } else if got != Err(M::InvalidHandle) {
            return Err(format!("stale handle {i}: {got:?}"));
        }
    }
    for (i, (expected, got)) in model.owned.iter().zip(&actual.owned).enumerate() {
        if expected.as_deref() != got.as_ref().map(OwnedBytes::as_slice) {
            return Err(format!("owned {i}"));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    pub seeds: u64,
    pub steps: u64,
    pub skipped: u64,
    pub operations: BTreeMap<String, u64>,
    pub errors: BTreeMap<String, u64>,
}

impl Coverage {
    fn record(&mut self, op: &Op, result: &str) {
        self.steps += 1;
        *self.operations.entry(op_name(op).into()).or_default() += 1;
        if result == "Skipped" {
            self.skipped += 1;
        }
        if result.starts_with("Err(") {
            *self.errors.entry(result.into()).or_default() += 1;
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.seeds += other.seeds;
        self.steps += other.steps;
        self.skipped += other.skipped;
        for (key, count) in &other.operations {
            *self.operations.entry(key.clone()).or_default() += count;
        }
        for (key, count) in &other.errors {
            *self.errors.entry(key.clone()).or_default() += count;
        }
    }

    pub fn to_json(
        &self,
        repeats: u32,
        seed_start: u64,
        seed_end_exclusive: Option<u64>,
    ) -> String {
        fn map(values: &BTreeMap<String, u64>) -> String {
            values
                .iter()
                .map(|(k, v)| format!("\"{k}\":{v}"))
                .collect::<Vec<_>>()
                .join(",")
        }
        let end = seed_end_exclusive.map_or_else(|| "null".into(), |value| value.to_string());
        let agreement = if repeats > 1 { "true" } else { "null" };
        format!("{{\"schema_version\":2,\"model_version\":5,\"seed_start\":{seed_start},\"seed_end_exclusive\":{end},\"seeds\":{},\"steps\":{},\"skipped\":{},\"operations\":{{{}}},\"errors\":{{{}}},\"repeats\":{repeats},\"repeated_run_agreement\":{agreement},\"dynamic_checks\":{{\"miri\":{{\"version\":null,\"result\":\"not_run\"}},\"sanitizer\":{{\"version\":null,\"result\":\"not_run\"}}}}}}",
            self.seeds, self.steps, self.skipped, map(&self.operations), map(&self.errors))
    }
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Create { .. } => "Create",
        Op::Alloc { .. } => "Alloc",
        Op::Owned { .. } => "Owned",
        Op::MutateOwned { .. } => "MutateOwned",
        Op::Promote { .. } => "Promote",
        Op::Copy { .. } => "Copy",
        Op::Root { .. } => "Root",
        Op::Unroot { .. } => "Unroot",
        Op::Link { .. } => "Link",
        Op::Read { .. } => "Read",
        Op::Collect { .. } => "Collect",
        Op::Close { .. } => "Close",
        Op::GuardStart { .. } => "GuardStart",
        Op::GuardEnd => "GuardEnd",
    }
}

/// Exact trace is deterministic for a seed. Prefix covers preflight, pressure,
/// nested guards, owned transfer, cycles, stale generations, and close paths.
pub fn generate(seed: u64) -> Vec<Op> {
    let len = (seed % 7) as usize;
    let charge = SLOT_METADATA_BYTES + len;
    let mut trace = vec![
        Op::Create {
            manual: false,
            limit: charge,
            trigger: charge,
        }, // region 1
        Op::Alloc { region: 1, len }, // handle 0
        Op::Alloc {
            region: 1,
            len: usize::MAX,
        }, // intrinsic impossibility
        Op::Alloc {
            region: 1,
            len: len + 1,
        }, // cannot fit even after collection
        Op::Read { handle: 0 },
        Op::Alloc { region: 1, len }, // handle 1, auto reclaims 0
        Op::Read { handle: 0 },
        Op::Root { handle: 1 },
        Op::Alloc { region: 1, len }, // auto collects but rooted 1 prevents allocation
        Op::Create {
            manual: false,
            limit: charge,
            trigger: 3 * charge,
        }, // region 2
        Op::Alloc { region: 2, len }, // handle 2
        Op::Alloc { region: 2, len }, // handle 3, pressure collection below trigger
        Op::Read { handle: 2 },
        Op::Create {
            manual: true,
            limit: charge,
            trigger: 0,
        }, // region 3
        Op::GuardStart {
            budget: charge * 2 + OWNED_METADATA_BYTES + len,
        },
        Op::GuardStart { budget: charge },
        Op::Alloc { region: 3, len }, // handle 4
        Op::Alloc { region: 3, len }, // manual failure; no collect
        Op::Alloc { region: 1, len }, // budget or region failure; no collect
        Op::Collect { region: 2 },    // forbidden even another region
        Op::GuardEnd,
        Op::Owned { len }, // owner 0
        Op::GuardEnd,
        Op::Unroot { handle: 1 },
        Op::Alloc { region: 1, len }, // handle 5, auto reclaims 1
        Op::Collect { region: 3 },
        Op::Read { handle: 4 },
        Op::Create {
            manual: true,
            limit: charge * 5 + 2 * EDGE_METADATA_BYTES,
            trigger: 0,
        }, // region 4
        Op::MutateOwned {
            owner: 0,
            byte: 0xa5,
        },
        Op::Promote {
            region: 1,
            owner: 0,
        }, // fails, owner remains
        Op::Promote {
            region: 4,
            owner: 0,
        }, // handle 6
        Op::Copy { handle: 6 }, // owner 1
        Op::MutateOwned {
            owner: 1,
            byte: 0x5a,
        },
        Op::Read { handle: 6 },
        Op::Alloc { region: 4, len }, // handle 7
        Op::Link {
            parent: 6,
            child: 7,
        },
        Op::Link {
            parent: 7,
            child: 6,
        },
        Op::Root { handle: 6 },
        Op::Close { region: 4 },   // roots block close
        Op::Collect { region: 4 }, // cycle survives
        Op::Unroot { handle: 6 },
        Op::Collect { region: 4 }, // cycle dies
        Op::Read { handle: 6 },
        Op::Alloc { region: 4, len }, // slot reused; handle 8
        Op::Close { region: 4 },
        Op::Read { handle: 8 },
    ];
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    while trace.len() + 3 <= MAX_STEPS {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let pick = (state >> 32) % 9;
        trace.push(match pick {
            0 => Op::Alloc { region: 1, len },
            1 => Op::Alloc { region: 3, len },
            2 => Op::Collect { region: 3 },
            3 => Op::Collect { region: 2 },
            4 => Op::Read { handle: 8 },
            5 => Op::Read { handle: 0 },
            6 => Op::Alloc {
                region: 2,
                len: len + 1,
            },
            7 => Op::Close { region: 3 },
            _ => Op::Copy { handle: 6 },
        });
    }
    trace
}

fn execute(
    session: &mut Session,
    actual: &mut Actual,
    model: &mut Model,
    trace: &[Op],
    offset: usize,
    coverage: &mut Coverage,
) -> Result<(), String> {
    let mut i = 0;
    while i < trace.len() {
        let op = &trace[i];
        if let Op::GuardStart { budget } = op {
            let mut depth = 1;
            let mut end = i + 1;
            while end < trace.len() && depth > 0 {
                match trace[end] {
                    Op::GuardStart { .. } => depth += 1,
                    Op::GuardEnd => depth -= 1,
                    _ => {}
                }
                end += 1;
            }
            if depth != 0 {
                return Err("invalid trace: unclosed guard".into());
            }
            let mut guard = session.no_gc(*budget);
            model.apply(op);
            coverage.record(op, "Ok");
            compare(&guard, actual, model)
                .map_err(|e| format!("step {} {op:?}: {e}", offset + i))?;
            execute(
                &mut guard,
                actual,
                model,
                &trace[i + 1..end - 1],
                offset + i + 1,
                coverage,
            )?;
            drop(guard);
            model.apply(&Op::GuardEnd);
            coverage.record(&Op::GuardEnd, "Ok");
            compare(session, actual, model)
                .map_err(|e| format!("step {} GuardEnd: {e}", offset + end - 1))?;
            i = end;
            continue;
        }
        if matches!(op, Op::GuardEnd) {
            return Err("invalid trace: unmatched guard end".into());
        }
        let expected = model.apply(op);
        let observed = actual.apply(session, op);
        if expected != observed {
            return Err(format!(
                "step {} {op:?}: expected {expected}, got {observed}",
                offset + i
            ));
        }
        compare(session, actual, model).map_err(|e| format!("step {} {op:?}: {e}", offset + i))?;
        coverage.record(op, &expected);
        i += 1;
    }
    Ok(())
}

pub fn run_trace(trace: &[Op], fault: bool) -> Result<Coverage, String> {
    let mut session = Session::new();
    let mut actual = Actual::new(&session);
    let mut model = Model::new(fault);
    let mut coverage = Coverage {
        seeds: 1,
        ..Coverage::default()
    };
    execute(
        &mut session,
        &mut actual,
        &mut model,
        trace,
        0,
        &mut coverage,
    )?;
    Ok(coverage)
}

pub fn run(seed: u64) -> Result<Coverage, String> {
    let trace = generate(seed);
    run_trace(&trace, false).map_err(|e| {
        format!(
            "model=5 seed={seed}: {e}\noriginal trace:\n{}\nminimized trace:\n{}",
            format_trace(&trace),
            format_trace(&shrink(&trace, false))
        )
    })
}

pub fn format_trace(trace: &[Op]) -> String {
    trace
        .iter()
        .enumerate()
        .map(|(i, op)| format!("{i:03}: {op:?}\n"))
        .collect()
}

/// Deletion reducer. Every candidate is replayed in a fresh session; only
/// valid traces with a per-step mismatch are retained.
pub fn shrink(trace: &[Op], fault: bool) -> Vec<Op> {
    fn mismatch(trace: &[Op], fault: bool) -> bool {
        run_trace(trace, fault).is_err_and(|e| e.starts_with("step "))
            && (!fault || run_trace(trace, false).is_ok())
    }
    if !mismatch(trace, fault) {
        return trace.to_vec();
    }
    let mut result = trace.to_vec();
    let mut span = result.len() / 2;
    while span > 0 {
        let mut i = 0;
        while i + span <= result.len() {
            let mut candidate = result.clone();
            candidate.drain(i..i + span);
            if mismatch(&candidate, fault) {
                result = candidate;
            } else {
                i += 1;
            }
        }
        span /= 2;
    }
    result
}
