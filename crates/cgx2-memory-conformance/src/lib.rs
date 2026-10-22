//! Bounded reference corpus for the frozen Wave 5 memory-script profile.
use std::collections::BTreeMap;

use cgx2_runtime::{CollectionMode, MemoryReport, SLOT_METADATA_BYTES};

pub mod v3;
pub mod v4;
pub mod v5;

pub const MODEL_VERSION: u32 = 1;
pub const MODEL_VERSION_V2: u32 = 2;
const REGION_LIMIT: usize = 4096;

/// Version 1 uses a 64-bit LCG: state = state * 6364136223846793005 + 1
/// (wrapping arithmetic), taking the upper 32 bits for each choice.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) as u32
    }

    fn choice(&mut self, count: usize) -> usize {
        (self.next() as usize) % count
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedRegion {
    pub open: bool,
    pub mode: &'static str,
    pub committed: usize,
    pub live: usize,
    pub peak: usize,
    pub collection_count: u64,
    pub collected: usize,
    pub failures: u64,
}

impl ExpectedRegion {
    fn new(mode: &'static str) -> Self {
        Self {
            open: true,
            mode,
            committed: 0,
            live: 0,
            peak: 0,
            collection_count: 0,
            collected: 0,
            failures: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedReport {
    pub regions: [ExpectedRegion; 2],
    pub guard_collection_deltas: Vec<u64>,
}

impl ExpectedReport {
    pub fn compare(&self, actual: &MemoryReport) -> Result<(), String> {
        if actual.regions.len() != 3 {
            return Err(format!(
                "expected default + 2 regions, got {}",
                actual.regions.len()
            ));
        }
        if actual.regions[0].collection_count != 0 || actual.regions[0].committed_bytes != 0 {
            return Err(format!("default region changed: {:?}", actual.regions[0]));
        }
        for (index, (expected, row)) in self.regions.iter().zip(&actual.regions[1..]).enumerate() {
            let mode = match row.mode {
                CollectionMode::Manual => "manual",
                CollectionMode::Auto {
                    trigger_bytes: 1_048_576,
                } => "auto",
                other => return Err(format!("region {index} unexpected mode {other:?}")),
            };
            let observed = (
                row.is_open,
                mode,
                row.committed_bytes,
                row.live_bytes,
                row.peak_bytes,
                row.collection_count,
                row.total_collected_bytes,
                row.allocation_failures,
            );
            let wanted = (
                expected.open,
                expected.mode,
                expected.committed,
                expected.live,
                expected.peak,
                expected.collection_count,
                expected.collected,
                expected.failures,
            );
            if observed != wanted {
                return Err(format!(
                    "region {index}: expected {wanted:?}, actual {observed:?}"
                ));
            }
            if row.live_bytes > row.committed_bytes || row.committed_bytes > row.peak_bytes {
                return Err(format!(
                    "region {index}: live <= committed <= peak violated"
                ));
            }
            if !row.is_open && (row.live_bytes != 0 || row.committed_bytes != 0) {
                return Err(format!("region {index}: closed with nonzero bytes"));
            }
            if expected.mode == "manual" && row.collection_count != expected.collection_count {
                return Err(format!("region {index}: manual collection count mismatch"));
            }
        }
        if self.guard_collection_deltas.iter().any(|delta| *delta != 0) {
            return Err("model collected inside no_gc".into());
        }
        let sums = self.regions.iter().fold((0, 0, 0, 0, 0, 0), |sum, row| {
            (
                sum.0 + row.committed,
                sum.1 + row.live,
                sum.2 + row.peak,
                sum.3 + row.collection_count,
                sum.4 + row.collected,
                sum.5 + row.failures,
            )
        });
        let totals = &actual.totals;
        if (
            totals.committed_bytes,
            totals.live_bytes,
            totals.peak_bytes,
            totals.collection_count,
            totals.total_collected_bytes,
            totals.allocation_failures,
        ) != sums
        {
            return Err(format!(
                "total counters mismatch: expected {sums:?}, actual {totals:?}"
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    pub seed: u64,
    pub source: String,
    pub expected: ExpectedReport,
    pub steps: usize,
    pub regions: usize,
    pub max_allocation: usize,
}

#[derive(Clone)]
struct Object {
    region: usize,
    charge: usize,
    rooted: bool,
}

struct Builder {
    source: String,
    regions: [ExpectedRegion; 2],
    objects: BTreeMap<&'static str, Object>,
    owned: BTreeMap<&'static str, usize>,
    borrowed: BTreeMap<&'static str, &'static str>,
    budgets: Vec<usize>,
    guard_starts: Vec<u64>,
    guard_deltas: Vec<u64>,
    depth: usize,
    steps: usize,
    max_allocation: usize,
}

impl Builder {
    fn new(a: &'static str, b: &'static str) -> Self {
        Self {
            source: String::new(),
            regions: [ExpectedRegion::new(a), ExpectedRegion::new(b)],
            objects: BTreeMap::new(),
            owned: BTreeMap::new(),
            borrowed: BTreeMap::new(),
            budgets: Vec::new(),
            guard_starts: Vec::new(),
            guard_deltas: Vec::new(),
            depth: 0,
            steps: 0,
            max_allocation: 0,
        }
    }

    fn line(&mut self, line: &str) {
        self.source.push_str(&"    ".repeat(self.depth));
        self.source.push_str(line);
        self.source.push('\n');
        self.steps += 1;
    }

    fn charge(&mut self, bytes: usize) {
        for remaining in &mut self.budgets {
            assert!(*remaining >= bytes, "generator exceeded a guard budget");
            *remaining -= bytes;
        }
    }

    fn count(&self) -> u64 {
        self.regions.iter().map(|r| r.collection_count).sum()
    }

    fn guard_begin(&mut self, budget: usize) {
        self.line(&format!("no_gc(budget: {budget}):"));
        self.guard_starts.push(self.count());
        self.budgets.push(budget);
        self.depth += 1;
    }

    fn guard_end(&mut self) {
        self.depth -= 1;
        self.budgets.pop().expect("guard stack");
        let before = self.guard_starts.pop().expect("guard stack");
        self.guard_deltas.push(self.count() - before);
    }

    fn open(&mut self, name: &'static str, index: usize) {
        self.line(&format!(
            "let {name} = memory.region(mode: {}, limit: {REGION_LIMIT})",
            self.regions[index].mode
        ));
    }

    fn owned(&mut self, name: &'static str, size: usize) {
        self.charge(32 + size);
        self.max_allocation = self.max_allocation.max(size);
        self.owned.insert(name, size);
        self.line(&format!("let {name} = memory.owned_bytes({size})"));
    }

    fn move_owned(&mut self, dest: &'static str, src: &'static str) {
        let size = self.owned.remove(src).expect("live owned source");
        self.owned.insert(dest, size);
        self.line(&format!("let {dest} = memory.move({src})"));
    }

    fn promote(&mut self, dest: &'static str, region_index: usize, src: &'static str) {
        assert!(!self.borrowed.values().any(|target| *target == src));
        let size = self.owned.remove(src).expect("live owned source");
        // The owned allocation already paid for its payload in active guards.
        self.charge(SLOT_METADATA_BYTES);
        let charge = SLOT_METADATA_BYTES + size;
        let region = &mut self.regions[region_index];
        assert!(region.open && region.committed + charge <= REGION_LIMIT);
        region.committed += charge;
        region.live += charge;
        region.peak = region.peak.max(region.committed);
        self.objects.insert(
            dest,
            Object {
                region: region_index,
                charge,
                rooted: true,
            },
        );
        let r = if region_index == 0 { "a" } else { "b" };
        self.line(&format!("let {dest} = memory.promote_owned({r}, {src})"));
    }

    fn copy_to_owned(&mut self, dest: &'static str, src: &'static str) {
        let object = self.objects.get(src).expect("live managed source");
        assert!(object.rooted);
        let size = object.charge - SLOT_METADATA_BYTES;
        self.charge(32 + size);
        self.owned.insert(dest, size);
        self.line(&format!("let {dest} = memory.copy_to_owned({src})"));
    }

    fn borrow(&mut self, token: &'static str, value: &'static str, mutable: bool) {
        assert!(
            self.owned.contains_key(value) || self.objects.get(value).is_some_and(|o| o.rooted)
        );
        self.borrowed.insert(token, value);
        let op = if mutable { "borrow_mut" } else { "borrow" };
        self.line(&format!("let {token} = memory.{op}({value})"));
    }

    fn end_borrow(&mut self, token: &'static str) {
        self.borrowed.remove(token).expect("live borrow");
        self.line(&format!("memory.end_borrow({token})"));
    }

    fn use_value(&mut self, name: &'static str) {
        assert!(self.owned.contains_key(name) || self.objects.get(name).is_some_and(|o| o.rooted));
        self.line(&format!("memory.use({name})"));
    }

    fn alloc(&mut self, name: &'static str, index: usize, size: usize) {
        let charge = SLOT_METADATA_BYTES + size;
        self.charge(charge);
        self.max_allocation = self.max_allocation.max(size);
        let region = &mut self.regions[index];
        assert!(region.open && region.committed + charge <= REGION_LIMIT);
        region.committed += charge;
        region.live += charge;
        region.peak = region.peak.max(region.committed);
        self.objects.insert(
            name,
            Object {
                region: index,
                charge,
                rooted: true,
            },
        );
        let r = if index == 0 { "a" } else { "b" };
        self.line(&format!("let {name} = memory.alloc_bytes({r}, {size})"));
    }

    fn drop_value(&mut self, name: &'static str) {
        assert!(!self.borrowed.values().any(|target| *target == name));
        if self.owned.remove(name).is_none() {
            self.objects
                .get_mut(name)
                .expect("live managed value")
                .rooted = false;
        }
        self.line(&format!("memory.drop({name})"));
    }

    fn collect(&mut self, index: usize) {
        assert!(self.budgets.is_empty());
        let region = &mut self.regions[index];
        assert!(region.open);
        let discarded: usize = self
            .objects
            .values()
            .filter(|o| o.region == index && !o.rooted)
            .map(|o| o.charge)
            .sum();
        self.objects.retain(|_, o| o.region != index || o.rooted);
        region.committed -= discarded;
        region.live -= discarded;
        region.collected += discarded;
        region.collection_count += 1;
        let name = if index == 0 { "a" } else { "b" };
        self.line(&format!("memory.collect({name})"));
    }

    fn close(&mut self, index: usize) {
        assert!(self
            .objects
            .values()
            .all(|o| o.region != index || !o.rooted));
        self.objects.retain(|_, o| o.region != index);
        let region = &mut self.regions[index];
        region.open = false;
        region.committed = 0;
        region.live = 0;
        let name = if index == 0 { "a" } else { "b" };
        self.line(&format!("memory.close({name})"));
    }
}

/// Produces at most 32 source statements, two regions, and 32-byte payloads.
pub fn generate(seed: u64) -> Case {
    let mut rng = Rng(seed);
    let mode_a = if rng.choice(2) == 0 { "auto" } else { "manual" };
    let mode_b = if rng.choice(2) == 0 { "auto" } else { "manual" };
    let mut b = Builder::new(mode_a, mode_b);
    b.open("a", 0);
    b.open("b", 1);
    b.owned("o", [8, 16, 32][rng.choice(3)]);
    b.move_owned("m", "o");
    b.use_value("m");
    b.borrow("token", "m", rng.choice(2) == 0);
    b.end_borrow("token");
    b.drop_value("m");
    b.alloc("x", 0, [8, 16, 32][rng.choice(3)]);
    b.guard_begin(512);
    b.alloc("y", 1, [8, 16, 32][rng.choice(3)]);
    b.guard_begin(256);
    b.alloc("z", 0, [8, 16, 32][rng.choice(3)]);
    b.owned("p", 8);
    b.move_owned("q", "p");
    b.drop_value("q");
    b.guard_end();
    b.guard_end();
    b.use_value("x");
    b.use_value("y");
    b.use_value("z");
    b.drop_value("y");
    b.collect(1);
    b.use_value("x");
    b.use_value("z");
    b.close(1);
    b.drop_value("x");
    if rng.choice(2) == 0 {
        b.collect(0);
    }
    b.drop_value("z");
    if rng.choice(2) == 0 {
        b.collect(0);
    }
    if rng.choice(2) == 0 {
        b.close(0);
    }
    assert!(b.depth == 0 && b.steps <= 48 && b.max_allocation <= 32);
    Case {
        seed,
        source: b.source,
        expected: ExpectedReport {
            regions: b.regions,
            guard_collection_deltas: b.guard_deltas,
        },
        steps: b.steps,
        regions: 2,
        max_allocation: b.max_allocation,
    }
}

/// Fixed checked-in seed corpus; CSV avoids a fuzz or JSON dependency.
pub fn fixed_seeds() -> Vec<u64> {
    include_str!("../corpus/seeds.csv")
        .lines()
        .skip(1)
        .map(|line| {
            let (version, seed) = line.split_once(',').expect("version,seed row");
            assert_eq!(
                version.parse::<u32>().expect("model version"),
                MODEL_VERSION
            );
            seed.parse::<u64>().expect("seed")
        })
        .collect()
}

/// Version 2 retains the v1 PRNG and independently models conversion charges.
/// Every generated successful script has at most 48 statements and 32-byte payloads.
pub fn generate_v2(seed: u64) -> Case {
    let mut rng = Rng(seed);
    let mode_a = if rng.choice(2) == 0 { "auto" } else { "manual" };
    let mode_b = if rng.choice(2) == 0 { "auto" } else { "manual" };
    let first_size = [0, 1, 8, 16, 32][rng.choice(5)];
    let second_size = [0, 1, 8, 16, 32][rng.choice(5)];
    let third_size = [0, 1, 8, 16, 32][rng.choice(5)];
    let mut b = Builder::new(mode_a, mode_b);
    b.open("a", 0);
    b.open("b", 1);
    b.owned("o", first_size);
    b.move_owned("m", "o");
    b.borrow("token", "m", rng.choice(2) == 0);
    b.end_borrow("token");
    b.guard_begin(512);
    b.promote("x", 0, "m");
    b.copy_to_owned("c", "x");
    b.guard_begin(256);
    b.owned("p", second_size);
    b.promote("z", 1, "p");
    b.alloc("y", 1, third_size);
    b.guard_end();
    b.guard_end();
    b.use_value("x");
    b.use_value("c");
    b.drop_value("c");
    b.drop_value("y");
    b.collect(1);
    b.use_value("z");
    b.drop_value("z");
    b.collect(1);
    b.close(1);
    b.drop_value("x");
    b.collect(0);
    b.close(0);
    assert!(b.depth == 0 && b.steps <= 48 && b.max_allocation <= 32);
    Case {
        seed,
        source: b.source,
        expected: ExpectedReport {
            regions: b.regions,
            guard_collection_deltas: b.guard_deltas,
        },
        steps: b.steps,
        regions: 2,
        max_allocation: b.max_allocation,
    }
}

pub fn fixed_seeds_v2() -> Vec<u64> {
    include_str!("../corpus/seeds-v2.csv")
        .lines()
        .skip(1)
        .map(|line| {
            let (version, seed) = line.split_once(',').expect("version,seed row");
            assert_eq!(
                version.parse::<u32>().expect("model version"),
                MODEL_VERSION_V2
            );
            seed.parse::<u64>().expect("seed")
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureCase {
    pub seed: u64,
    pub source: String,
    pub expected_code: &'static str,
    pub expected_error: &'static str,
}

/// Three dynamically failing, statically valid one-operation mutations per seed.
/// A source failure has no successful report; runtime preservation is tested
/// against `Session` separately.
pub fn generate_v2_failures(seed: u64) -> [FailureCase; 3] {
    let size = [0, 1, 8, 16, 32][(seed % 5) as usize];
    [
        FailureCase {
            seed,
            source: format!("let r = memory.region(mode: auto, limit: 4096)\nlet o = memory.owned_bytes({size})\nno_gc(budget: 63):\n    let m = memory.promote_owned(r, o)\n"),
            expected_code: "CGR013",
            expected_error: "BudgetExceeded",
        },
        FailureCase {
            seed,
            source: format!("let r = memory.region(mode: manual, limit: {})\nlet o = memory.owned_bytes({size})\nlet m = memory.promote_owned(r, o)\n", 63 + size),
            expected_code: "CGR013",
            expected_error: "RegionLimit",
        },
        FailureCase {
            seed,
            source: format!("let r = memory.region(mode: manual, limit: 4096)\nlet m = memory.alloc_bytes(r, {size})\nno_gc(budget: 31):\n    let c = memory.copy_to_owned(m)\n"),
            expected_code: "CGR013",
            expected_error: "BudgetExceeded",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_tracks_roots_collection_isolation_and_close() {
        let mut builder = Builder::new("manual", "auto");
        builder.open("a", 0);
        builder.open("b", 1);
        builder.alloc("x", 0, 8);
        builder.alloc("y", 1, 16);
        builder.drop_value("x");
        builder.collect(0);
        assert_eq!(builder.regions[0].committed, 0);
        assert_eq!(builder.regions[0].collected, SLOT_METADATA_BYTES + 8);
        assert_eq!(builder.regions[0].collection_count, 1);
        assert_eq!(builder.regions[1].committed, SLOT_METADATA_BYTES + 16);
        builder.use_value("y");
        builder.drop_value("y");
        builder.close(1);
        assert_eq!(builder.regions[1].committed, 0);
        assert_eq!(builder.regions[1].peak, SLOT_METADATA_BYTES + 16);
        assert_eq!(builder.regions[1].collection_count, 0);
    }

    #[test]
    fn fixed_seeds_cover_modes_and_open_closed_states() {
        let mut pairs = std::collections::BTreeSet::new();
        let mut open_states = std::collections::BTreeSet::new();
        for seed in fixed_seeds() {
            let case = generate(seed);
            pairs.insert((case.expected.regions[0].mode, case.expected.regions[1].mode));
            open_states.insert(case.expected.regions[0].open);
        }
        assert_eq!(pairs.len(), 4);
        assert_eq!(open_states.len(), 2);
    }

    #[test]
    fn v2_model_promotion_moves_owner_and_copy_leaves_managed_root() {
        let mut builder = Builder::new("manual", "auto");
        builder.open("a", 0);
        builder.open("b", 1);
        builder.owned("o", 0);
        builder.guard_begin(96);
        builder.promote("x", 0, "o");
        builder.copy_to_owned("c", "x");
        assert!(!builder.owned.contains_key("o"));
        assert_eq!(builder.owned.get("c"), Some(&0));
        assert!(builder.objects.get("x").is_some_and(|object| object.rooted));
        assert_eq!(builder.budgets, vec![0]);
        assert_eq!(builder.regions[0].committed, SLOT_METADATA_BYTES);
        assert_eq!(builder.regions[0].peak, SLOT_METADATA_BYTES);
        assert_eq!(builder.regions[1].committed, 0);
        builder.guard_end();
        builder.drop_value("x");
        builder.collect(0);
        assert_eq!(builder.regions[0].collected, SLOT_METADATA_BYTES);
        assert_eq!(builder.regions[0].collection_count, 1);
    }
}
