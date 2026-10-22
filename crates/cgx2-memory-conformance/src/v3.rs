//! Direct-runtime, bounded state-machine corpus. Version 3 is independent of the
//! source-script models; changing its PRNG or transition rules requires a new version.
use cgx2_runtime::{
    AllocationError, CollectionMode, Handle, MemoryError, OwnedBytes, RegionId, RegionStats,
    Session, EDGE_METADATA_BYTES, OWNED_METADATA_BYTES, REGION_METADATA_BYTES, SLOT_METADATA_BYTES,
};

pub const MODEL_VERSION_V3: u32 = 3;
const MAX_STEPS: usize = 96;
const MAX_PAYLOAD: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Create { manual: bool, limit: usize },
    Alloc { region: usize, len: usize },
    Owned { len: usize },
    MutateOwned { owner: usize, byte: u8 },
    Promote { region: usize, owner: usize },
    Copy { handle: usize },
    Root { handle: usize },
    Unroot { handle: usize },
    Link { parent: usize, child: usize },
    Read { handle: usize },
    Collect { region: usize },
    Close { region: usize },
    ForeignRead,
    ForeignRegionAlloc,
    GuardStart { budget: usize },
    GuardEnd,
}

impl Op {
    pub fn payload_len(&self) -> Option<usize> {
        match self {
            Self::Alloc { len, .. } | Self::Owned { len } => Some(*len),
            _ => None,
        }
    }
}

// Same documented LCG family as v1/v2. Every choice takes upper 32 bits.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) as u32
    }
    fn choice(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

/// At most 96 operations, three total regions, two nested guards and 32-byte
/// payloads. The fixed prefix deliberately reaches invalid and boundary paths.
pub fn generate(seed: u64) -> Vec<Op> {
    let mut rng = Rng(seed);
    let mut ops = vec![
        Op::GuardStart { budget: 127 },
        Op::Create {
            manual: true,
            limit: 128,
        }, // region metadata exceeds guard budget
        Op::GuardEnd,
        Op::Create {
            manual: true,
            limit: 256,
        },
        Op::Create {
            manual: false,
            limit: 512,
        },
        Op::Alloc { region: 1, len: 0 },
        Op::Root { handle: 0 },
        Op::Alloc { region: 2, len: 32 },
        Op::Link {
            parent: 0,
            child: 1,
        }, // cross-region
        Op::ForeignRead,
        Op::ForeignRegionAlloc,
        Op::Owned { len: 0 },
        Op::GuardStart { budget: 128 },
        Op::GuardStart { budget: 63 },
        Op::Promote {
            region: 1,
            owner: 0,
        }, // budget exhaustion preserves owner
        Op::Alloc { region: 1, len: 0 }, // budget exhaustion
        Op::Collect { region: 2 },       // forbidden even for another region
        Op::GuardEnd,
        Op::GuardEnd,
        Op::Promote {
            region: 1,
            owner: 0,
        },
        Op::Alloc { region: 1, len: 32 },
        Op::Alloc { region: 1, len: 0 }, // near-limit failure before commitment
        Op::Unroot { handle: 0 },
        Op::Collect { region: 1 },
        Op::Read { handle: 0 }, // stale
        Op::Owned { len: 1 },
        Op::MutateOwned {
            owner: 1,
            byte: 0xa5,
        },
        Op::Promote {
            region: 2,
            owner: 1,
        },
        Op::Copy { handle: 4 },
        Op::MutateOwned {
            owner: 2,
            byte: 0x5a,
        },
        Op::Read { handle: 4 }, // copy did not alias managed bytes
    ];
    let mut depth = 0;
    // Reserve one slot for this choice and one for a possible new guard end.
    while ops.len() + depth + 2 <= MAX_STEPS {
        let pick = rng.choice(20);
        let region = rng.choice(3);
        let handle = rng.choice(16);
        let len = [0, 1, 7, 31, MAX_PAYLOAD][rng.choice(5)];
        let op = match pick {
            0 | 1 => Op::Alloc { region, len },
            2 => Op::Owned { len },
            19 => Op::MutateOwned {
                owner: rng.choice(8),
                byte: rng.next() as u8,
            },
            3 => Op::Promote {
                region,
                owner: rng.choice(8),
            },
            4 => Op::Copy { handle },
            5 => Op::Root { handle },
            6 => Op::Unroot { handle },
            7 | 8 => Op::Link {
                parent: handle,
                child: rng.choice(16),
            },
            9 => Op::Read { handle },
            10 | 11 => Op::Collect { region },
            12 => Op::Close { region },
            13 => Op::ForeignRead,
            14 => Op::ForeignRegionAlloc,
            15 if depth < 2 => {
                depth += 1;
                Op::GuardStart {
                    budget: [0, 63, 64, 128, 256][rng.choice(5)],
                }
            }
            16 if depth > 0 => {
                depth -= 1;
                Op::GuardEnd
            }
            _ => Op::Alloc { region, len },
        };
        ops.push(op);
    }
    ops.extend(std::iter::repeat_n(Op::GuardEnd, depth));
    ops
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    OvercountFirstAllocation,
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
    fault: Fault,
    fault_applied: bool,
}
impl Model {
    fn new(fault: Fault) -> Self {
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
    fn budget_check(&self, charge: usize) -> Result<(), AllocationError> {
        if self
            .budgets
            .iter()
            .any(|(limit, spent)| charge > limit.saturating_sub(*spent))
        {
            Err(AllocationError::BudgetExceeded)
        } else {
            Ok(())
        }
    }
    fn charge(&mut self, bytes: usize) {
        for (_, spent) in &mut self.budgets {
            *spent += bytes;
        }
    }
    fn region(&self, index: usize) -> Result<&Region, MemoryError> {
        self.regions
            .get(index)
            .filter(|r| r.open)
            .ok_or(MemoryError::InvalidRegion)
    }
    fn object(&self, index: usize) -> Result<&Object, MemoryError> {
        self.objects
            .get(index)
            .filter(|o| o.valid && self.regions[o.region].open)
            .ok_or(MemoryError::InvalidHandle)
    }
    fn object_mut(&mut self, index: usize) -> Result<&mut Object, MemoryError> {
        if self.object(index).is_err() {
            return Err(MemoryError::InvalidHandle);
        }
        Ok(&mut self.objects[index])
    }
    fn collect(&mut self, region: usize) {
        let mut marked = vec![false; self.objects.len()];
        for (index, object) in self.objects.iter().enumerate() {
            if object.valid && object.region == region && object.roots > 0 {
                marked[index] = true;
            }
        }
        loop {
            let mut changed = false;
            for index in 0..self.objects.len() {
                if !marked[index] {
                    continue;
                }
                for &child in &self.objects[index].edges {
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
        for (index, object) in self.objects.iter_mut().enumerate() {
            if object.valid && object.region == region && !marked[index] {
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
    fn alloc(&mut self, region: usize, len: usize) -> Result<(), AllocationError> {
        let r = self
            .region(region)
            .map_err(|_| AllocationError::InvalidRegion)?;
        if self.budgets.is_empty()
            && matches!(r.mode, CollectionMode::Auto { trigger_bytes } if r.stats.committed_bytes >= trigger_bytes)
        {
            self.collect(region);
        }
        let charge = SLOT_METADATA_BYTES + len;
        let error = self.budget_check(charge).err().or_else(|| {
            (charge
                > self.regions[region]
                    .limit
                    .saturating_sub(self.regions[region].stats.committed_bytes))
            .then_some(AllocationError::RegionLimit)
        });
        if let Some(error) = error {
            self.regions[region].stats.allocation_failures += 1;
            return Err(error);
        }
        let stats = &mut self.regions[region].stats;
        stats.committed_bytes += charge;
        stats.live_bytes += charge;
        stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
        self.charge(charge);
        self.objects.push(Object {
            region,
            bytes: vec![0; len],
            roots: 0,
            edges: Vec::new(),
            charge,
            valid: true,
        });
        if self.fault == Fault::OvercountFirstAllocation && !self.fault_applied {
            self.regions[region].stats.committed_bytes += 1;
            self.fault_applied = true;
        }
        Ok(())
    }
    fn apply(&mut self, op: &Op) -> String {
        match *op {
            Op::Create { manual, limit } => {
                let result = self.budget_check(REGION_METADATA_BYTES);
                if result.is_ok() {
                    self.regions.push(Region {
                        open: true,
                        mode: if manual {
                            CollectionMode::Manual
                        } else {
                            CollectionMode::Auto { trigger_bytes: 256 }
                        },
                        limit,
                        stats: RegionStats::default(),
                    });
                    self.charge(REGION_METADATA_BYTES);
                }
                outcome(result)
            }
            Op::Alloc { region, len } => outcome(self.alloc(region, len)),
            Op::Owned { len } => {
                let result = self.budget_check(OWNED_METADATA_BYTES + len);
                if result.is_ok() {
                    self.charge(OWNED_METADATA_BYTES + len);
                    self.owned.push(Some(vec![0; len]));
                }
                outcome(result)
            }
            Op::MutateOwned { owner, byte } => {
                if self.owned.is_empty() {
                    return "Skipped".into();
                }
                let owner = owner % self.owned.len();
                let Some(bytes) = self.owned[owner].as_mut() else {
                    return "Skipped".into();
                };
                if let Some(first) = bytes.first_mut() {
                    *first = byte;
                }
                "Ok".into()
            }
            Op::Promote { region, owner } => {
                if self.owned.is_empty() {
                    return "Skipped".into();
                }
                let owner = owner % self.owned.len();
                let Some(bytes) = self.owned[owner].as_ref() else {
                    return "Skipped".into();
                };
                if self.region(region).is_err() {
                    return outcome(Err(AllocationError::InvalidRegion));
                }
                let charge = SLOT_METADATA_BYTES + bytes.len();
                let error = self.budget_check(SLOT_METADATA_BYTES).err().or_else(|| {
                    (charge
                        > self.regions[region]
                            .limit
                            .saturating_sub(self.regions[region].stats.committed_bytes))
                    .then_some(AllocationError::RegionLimit)
                });
                if let Some(error) = error {
                    self.regions[region].stats.allocation_failures += 1;
                    return outcome(Err(error));
                }
                let bytes = self.owned[owner].take().unwrap();
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
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                let handle = handle % self.objects.len();
                let bytes = match self.object(handle) {
                    Ok(o) => o.bytes.clone(),
                    Err(e) => return outcome(Err(e)),
                };
                let result = self
                    .budget_check(OWNED_METADATA_BYTES + bytes.len())
                    .map_err(MemoryError::Allocation);
                if result.is_ok() {
                    self.charge(OWNED_METADATA_BYTES + bytes.len());
                    self.owned.push(Some(bytes));
                }
                outcome(result)
            }
            Op::Root { handle } => {
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                let result = self
                    .object_mut(handle % self.objects.len())
                    .map(|o| o.roots += 1);
                outcome(result)
            }
            Op::Unroot { handle } => {
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                let result = self.object_mut(handle % self.objects.len()).and_then(|o| {
                    if o.roots == 0 {
                        Err(MemoryError::NotRooted)
                    } else {
                        o.roots -= 1;
                        Ok(())
                    }
                });
                outcome(result)
            }
            Op::Link { parent, child } => {
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                let parent = parent % self.objects.len();
                let child = child % self.objects.len();
                let p = match self.object(parent) {
                    Ok(p) => p,
                    Err(e) => return outcome(Err(e)),
                };
                let c = match self.object(child) {
                    Ok(c) => c,
                    Err(e) => return outcome(Err(e)),
                };
                if p.region != c.region {
                    return outcome(Err(MemoryError::CrossRegionEdge));
                }
                if p.edges.contains(&child) {
                    return "Ok".into();
                }
                let region = p.region;
                // The runtime reserves an exact replacement vector of length old_len + 1.
                let old_capacity = p.edges.capacity();
                if p.edges.len() == old_capacity {
                    let new_capacity = p.edges.len() + 1;
                    let added = (new_capacity - old_capacity) * EDGE_METADATA_BYTES;
                    let budget_charge = new_capacity * EDGE_METADATA_BYTES;
                    let error = self.budget_check(budget_charge).err().or_else(|| {
                        (added
                            > self.regions[region]
                                .limit
                                .saturating_sub(self.regions[region].stats.committed_bytes))
                        .then_some(AllocationError::RegionLimit)
                    });
                    if let Some(error) = error {
                        self.regions[region].stats.allocation_failures += 1;
                        return outcome(Err(MemoryError::Allocation(error)));
                    }
                    let mut edges = Vec::with_capacity(new_capacity);
                    edges.extend_from_slice(&self.objects[parent].edges);
                    edges.push(child);
                    self.objects[parent].edges = edges;
                    self.objects[parent].charge += added;
                    let stats = &mut self.regions[region].stats;
                    stats.committed_bytes += added;
                    stats.live_bytes += added;
                    stats.peak_bytes = stats.peak_bytes.max(stats.committed_bytes);
                    self.charge(budget_charge);
                } else {
                    self.objects[parent].edges.push(child);
                }
                "Ok".into()
            }
            Op::Read { handle } => {
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                match self.object(handle % self.objects.len()) {
                    Ok(o) => format!("Ok({:?})", o.bytes),
                    Err(e) => outcome(Err(e)),
                }
            }
            Op::Collect { region } => {
                if !self.budgets.is_empty() {
                    return outcome(Err(MemoryError::CollectionForbidden));
                }
                if let Err(e) = self.region(region) {
                    return outcome(Err(e));
                }
                self.collect(region);
                "Ok".into()
            }
            Op::Close { region } => {
                if let Err(e) = self.region(region) {
                    return outcome(Err(e));
                }
                if self
                    .objects
                    .iter()
                    .any(|o| o.valid && o.region == region && o.roots > 0)
                {
                    return outcome(Err(MemoryError::RegionHasRoots));
                }
                self.regions[region].open = false;
                self.regions[region].stats.committed_bytes = 0;
                self.regions[region].stats.live_bytes = 0;
                for o in &mut self.objects {
                    if o.region == region {
                        o.valid = false;
                    }
                }
                "Ok".into()
            }
            Op::ForeignRead => outcome(Err(MemoryError::InvalidHandle)),
            Op::ForeignRegionAlloc => outcome(Err(AllocationError::InvalidRegion)),
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
}
fn outcome<T: std::fmt::Debug>(result: Result<(), T>) -> String {
    match result {
        Ok(()) => "Ok".into(),
        Err(error) => format!("Err({error:?})"),
    }
}

struct Actual {
    regions: Vec<RegionId>,
    objects: Vec<Handle>,
    owned: Vec<Option<OwnedBytes>>,
    foreign_region: RegionId,
    foreign_handle: Handle,
}
impl Actual {
    fn new(session: &Session) -> Self {
        let mut foreign = Session::new();
        let foreign_region = foreign.default_region();
        let foreign_handle = foreign.alloc_bytes(foreign_region, 1).unwrap();
        Self {
            regions: vec![session.default_region()],
            objects: Vec::new(),
            owned: Vec::new(),
            foreign_region,
            foreign_handle,
        }
    }
    fn apply(&mut self, session: &mut Session, op: &Op) -> String {
        match *op {
            Op::Create { manual, limit } => {
                let mode = if manual {
                    CollectionMode::Manual
                } else {
                    CollectionMode::Auto { trigger_bytes: 256 }
                };
                match session.create_region(mode, limit) {
                    Ok(id) => {
                        self.regions.push(id);
                        "Ok".into()
                    }
                    Err(e) => outcome(Err(e)),
                }
            }
            Op::Alloc { region, len } => {
                let id = self
                    .regions
                    .get(region)
                    .copied()
                    .unwrap_or(self.foreign_region);
                match session.alloc_bytes(id, len) {
                    Ok(h) => {
                        self.objects.push(h);
                        "Ok".into()
                    }
                    Err(e) => outcome(Err(e)),
                }
            }
            Op::Owned { len } => match session.alloc_owned_bytes(len) {
                Ok(o) => {
                    self.owned.push(Some(o));
                    "Ok".into()
                }
                Err(e) => outcome(Err(e)),
            },
            Op::MutateOwned { owner, byte } => {
                if self.owned.is_empty() {
                    return "Skipped".into();
                }
                let owner = owner % self.owned.len();
                let Some(value) = self.owned[owner].as_mut() else {
                    return "Skipped".into();
                };
                if let Some(first) = value.as_mut_slice().first_mut() {
                    *first = byte;
                }
                "Ok".into()
            }
            Op::Promote { region, owner } => {
                if self.owned.is_empty() {
                    return "Skipped".into();
                }
                let owner = owner % self.owned.len();
                let Some(value) = self.owned[owner].take() else {
                    return "Skipped".into();
                };
                let id = self
                    .regions
                    .get(region)
                    .copied()
                    .unwrap_or(self.foreign_region);
                match session.promote_owned(id, value) {
                    Ok(h) => {
                        self.objects.push(h);
                        "Ok".into()
                    }
                    Err((e, value)) => {
                        self.owned[owner] = Some(value);
                        outcome(Err(e))
                    }
                }
            }
            Op::Copy { handle } => {
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                match session.copy_managed_to_owned(self.objects[handle % self.objects.len()]) {
                    Ok(o) => {
                        self.owned.push(Some(o));
                        "Ok".into()
                    }
                    Err(e) => outcome(Err(e)),
                }
            }
            Op::Root { handle } => {
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                outcome(session.root(self.objects[handle % self.objects.len()]))
            }
            Op::Unroot { handle } => {
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                outcome(session.unroot(self.objects[handle % self.objects.len()]))
            }
            Op::Link { parent, child } => {
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                outcome(session.link(
                    self.objects[parent % self.objects.len()],
                    self.objects[child % self.objects.len()],
                ))
            }
            Op::Read { handle } => {
                if self.objects.is_empty() {
                    return "Skipped".into();
                }
                match session.read_bytes(self.objects[handle % self.objects.len()]) {
                    Ok(b) => format!("Ok({b:?})"),
                    Err(e) => outcome(Err(e)),
                }
            }
            Op::Collect { region } => outcome(
                session.collect(
                    self.regions
                        .get(region)
                        .copied()
                        .unwrap_or(self.foreign_region),
                ),
            ),
            Op::Close { region } => outcome(
                session.close_region(
                    self.regions
                        .get(region)
                        .copied()
                        .unwrap_or(self.foreign_region),
                ),
            ),
            Op::ForeignRead => outcome(session.read_bytes(self.foreign_handle).map(|_| ())),
            Op::ForeignRegionAlloc => {
                outcome(session.alloc_bytes(self.foreign_region, 1).map(|_| ()))
            }
            Op::GuardStart { .. } | Op::GuardEnd => "Ok".into(),
        }
    }
}

fn compare(session: &Session, actual: &Actual, model: &Model) -> Result<(), String> {
    let report = session.memory_report();
    if report.regions.len() != model.regions.len() {
        return Err(format!(
            "region count: expected {}, got {}",
            model.regions.len(),
            report.regions.len()
        ));
    }
    for (index, (expected, observed)) in model.regions.iter().zip(&report.regions).enumerate() {
        if observed.is_open != expected.open
            || observed.mode != expected.mode
            || observed.limit_bytes != expected.limit
            || observed.committed_bytes != expected.stats.committed_bytes
            || observed.live_bytes != expected.stats.live_bytes
            || observed.peak_bytes != expected.stats.peak_bytes
            || observed.allocation_failures != expected.stats.allocation_failures
            || observed.collection_count != expected.stats.collection_count
            || observed.total_collected_bytes != expected.stats.total_collected_bytes
        {
            return Err(format!("region {index}: expected open={} mode={:?} limit={} stats={:?}; got open={} mode={:?} limit={} stats={:?}", expected.open, expected.mode, expected.limit, expected.stats, observed.is_open, observed.mode, observed.limit_bytes, RegionStats { committed_bytes: observed.committed_bytes, live_bytes: observed.live_bytes, peak_bytes: observed.peak_bytes, allocation_failures: observed.allocation_failures, collection_count: observed.collection_count, total_collected_bytes: observed.total_collected_bytes }));
        }
        if observed.live_bytes > observed.committed_bytes
            || observed.committed_bytes > observed.peak_bytes
            || observed.committed_bytes > observed.limit_bytes && observed.is_open
        {
            return Err(format!("region {index}: accounting invariant violated"));
        }
        if observed.pause.sample_count != observed.collection_count {
            return Err(format!("region {index}: pause sample count mismatch"));
        }
    }
    for (index, object) in model.objects.iter().enumerate() {
        let observed = session.read_bytes(actual.objects[index]);
        if object.valid && model.regions[object.region].open {
            if observed != Ok(object.bytes.as_slice()) {
                return Err(format!(
                    "handle {index}: expected {:?}, got {observed:?}",
                    object.bytes
                ));
            }
        } else if observed != Err(MemoryError::InvalidHandle) {
            return Err(format!("stale handle {index}: got {observed:?}"));
        }
    }
    for (index, owned) in model.owned.iter().enumerate() {
        match (owned, &actual.owned[index]) {
            (Some(expected), Some(observed)) if expected == observed.as_slice() => {}
            (None, None) => {}
            _ => {
                return Err(format!(
                    "owned value {index} changed or was consumed on failure"
                ))
            }
        }
    }
    let summed = model
        .regions
        .iter()
        .fold(RegionStats::default(), |mut total, r| {
            total.committed_bytes += r.stats.committed_bytes;
            total.live_bytes += r.stats.live_bytes;
            total.peak_bytes += r.stats.peak_bytes;
            total.allocation_failures += r.stats.allocation_failures;
            total.collection_count += r.stats.collection_count;
            total.total_collected_bytes += r.stats.total_collected_bytes;
            total
        });
    if report.totals != summed {
        return Err(format!(
            "totals: expected {summed:?}, got {:?}",
            report.totals
        ));
    }
    Ok(())
}

fn execute(
    session: &mut Session,
    actual: &mut Actual,
    model: &mut Model,
    ops: &[Op],
    base: usize,
) -> Result<(), String> {
    let mut index = 0;
    while index < ops.len() {
        let op = &ops[index];
        if let Op::GuardStart { budget } = op {
            let mut depth = 1;
            let mut end = index + 1;
            while end < ops.len() && depth > 0 {
                match ops[end] {
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
            compare(&guard, actual, model)
                .map_err(|e| format!("step {} {op:?}: {e}", base + index))?;
            execute(
                &mut guard,
                actual,
                model,
                &ops[index + 1..end - 1],
                base + index + 1,
            )?;
            drop(guard);
            model.apply(&Op::GuardEnd);
            compare(session, actual, model)
                .map_err(|e| format!("step {} GuardEnd: {e}", base + end - 1))?;
            index = end;
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
                base + index
            ));
        }
        compare(session, actual, model)
            .map_err(|e| format!("step {} {op:?}: {e}", base + index))?;
        index += 1;
    }
    Ok(())
}

pub fn run_trace(trace: &[Op], fault: Fault) -> Result<(), String> {
    let mut session = Session::new();
    let mut actual = Actual::new(&session);
    let mut model = Model::new(fault);
    execute(&mut session, &mut actual, &mut model, trace, 0)
}
pub fn run(seed: u64, fault: Fault) -> Result<(), String> {
    let trace = generate(seed);
    run_trace(&trace, fault).map_err(|e| {
        format!(
            "model=3 seed={seed}: {e}\noriginal trace:\n{}\nminimized trace:\n{}",
            format_trace(&trace),
            format_trace(&shrink(&trace, fault))
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
fn mismatch(trace: &[Op], fault: Fault) -> bool {
    run_trace(trace, fault).is_err_and(|e| e.starts_with("step "))
}
/// Delta-debug by deletion, then reduce payload sizes to zero or one. Replays
/// each candidate in a fresh session and keeps only real per-step mismatches.
pub fn shrink(trace: &[Op], fault: Fault) -> Vec<Op> {
    if !mismatch(trace, fault) {
        return trace.to_vec();
    }
    let mut result = trace.to_vec();
    let mut span = result.len() / 2;
    while span > 0 {
        let mut index = 0;
        while index + span <= result.len() {
            let mut candidate = result.clone();
            candidate.drain(index..index + span);
            if mismatch(&candidate, fault) {
                result = candidate;
            } else {
                index += 1;
            }
        }
        span /= 2;
    }
    for index in 0..result.len() {
        for len in [0, 1] {
            let mut candidate = result.clone();
            match &mut candidate[index] {
                Op::Alloc { len: current, .. } | Op::Owned { len: current } if len < *current => {
                    *current = len
                }
                _ => continue,
            }
            if mismatch(&candidate, fault) {
                result = candidate;
            }
        }
    }
    result
}
