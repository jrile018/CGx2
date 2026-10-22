//! Version 4 reuses the independently checked v3 transitions, with a separate
//! bounded generator, exact manifest, and observed operation/error coverage.
use std::collections::BTreeMap;

use crate::v3::{self, Fault, Op};

pub const MODEL_VERSION_V4: u32 = 4;
pub const REQUIRED_OPERATIONS: &[&str] = &[
    "Create",
    "Alloc",
    "Owned",
    "MutateOwned",
    "Promote",
    "Copy",
    "Root",
    "Unroot",
    "Link",
    "Read",
    "Collect",
    "Close",
    "ForeignRead",
    "ForeignRegionAlloc",
    "GuardStart",
    "GuardEnd",
];
pub const REQUIRED_ERRORS: &[&str] = &[
    "Err(BudgetExceeded)",
    "Err(RegionLimit)",
    "Err(CrossRegionEdge)",
    "Err(InvalidHandle)",
    "Err(InvalidRegion)",
    "Err(CollectionForbidden)",
    "Err(RegionHasRoots)",
    "Err(NotRooted)",
];
pub const REQUIRED_FAMILIES: &[&str] = &[
    "stale_after_slot_reuse",
    "roots_through_links",
    "cross_region_link",
    "failed_move_keeps_owner",
    "independent_copy",
    "zero_length_region_limit",
    "nested_guard_failure",
    "auto_after_guard",
    "wrong_session_handle",
    "wrong_session_region",
    "close_recreate_isolation",
    "close_with_roots",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    pub seeds: u64,
    pub steps: u64,
    pub skipped: u64,
    pub skipped_by_operation: BTreeMap<String, u64>,
    pub operations: BTreeMap<String, u64>,
    pub outcomes: BTreeMap<String, u64>,
    pub families: BTreeMap<String, u64>,
}

impl Coverage {
    fn record(&mut self, op: &Op, outcome: &str) {
        self.steps += 1;
        self.skipped += u64::from(outcome == "Skipped");
        let operation = name(op);
        *self.operations.entry(operation.into()).or_default() += 1;
        if outcome == "Skipped" {
            *self
                .skipped_by_operation
                .entry(operation.into())
                .or_default() += 1;
        }
        let result = if outcome.starts_with("Ok([") {
            "Ok(bytes)"
        } else {
            outcome
        };
        *self.outcomes.entry(result.into()).or_default() += 1;
    }

    pub fn merge(&mut self, other: &Self) {
        self.seeds += other.seeds;
        self.steps += other.steps;
        self.skipped += other.skipped;
        for (key, count) in &other.skipped_by_operation {
            *self.skipped_by_operation.entry(key.clone()).or_default() += count;
        }
        for (key, count) in &other.operations {
            *self.operations.entry(key.clone()).or_default() += count;
        }
        for (key, count) in &other.outcomes {
            *self.outcomes.entry(key.clone()).or_default() += count;
        }
        for (key, count) in &other.families {
            *self.families.entry(key.clone()).or_default() += count;
        }
    }

    /// All keys are fixed operation/result labels emitted by this crate.
    pub fn to_json(&self) -> String {
        fn map(values: &BTreeMap<String, u64>) -> String {
            values
                .iter()
                .map(|(key, value)| format!("\"{key}\":{value}"))
                .collect::<Vec<_>>()
                .join(",")
        }
        format!(
            "{{\"schema_version\":1,\"model_version\":4,\"seeds\":{},\"steps\":{},\"skipped\":{},\"skipped_by_operation\":{{{}}},\"operations\":{{{}}},\"outcomes\":{{{}}},\"families\":{{{}}}}}",
            self.seeds,
            self.steps,
            self.skipped,
            map(&self.skipped_by_operation),
            map(&self.operations),
            map(&self.outcomes),
            map(&self.families)
        )
    }
}

fn name(op: &Op) -> &'static str {
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
        Op::ForeignRead => "ForeignRead",
        Op::ForeignRegionAlloc => "ForeignRegionAlloc",
        Op::GuardStart { .. } => "GuardStart",
        Op::GuardEnd => "GuardEnd",
    }
}

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

// Indices are deliberately stable: family counters below require both a
// particular adversarial operation and its independently observed outcome.
fn prefix() -> Vec<Op> {
    vec![
        Op::Copy { handle: 0 }, // 0: explicit skip
        Op::GuardStart { budget: 127 },
        Op::Create {
            manual: true,
            limit: 128,
        },
        Op::GuardEnd,
        Op::Create {
            manual: true,
            limit: 256,
        }, // region 1
        Op::Create {
            manual: false,
            limit: 512,
        }, // region 2
        Op::Alloc { region: 1, len: 0 }, // handle 0
        Op::Root { handle: 0 },
        Op::Alloc { region: 1, len: 0 }, // handle 1
        Op::Link {
            parent: 0,
            child: 1,
        },
        Op::Collect { region: 1 },
        Op::Read { handle: 1 },
        Op::Unroot { handle: 1 },         // NotRooted
        Op::Alloc { region: 2, len: 32 }, // handle 2
        Op::Link {
            parent: 0,
            child: 2,
        },
        Op::ForeignRead,
        Op::ForeignRegionAlloc,
        Op::Owned { len: 0 }, // owner 0
        Op::GuardStart { budget: 128 },
        Op::GuardStart { budget: 63 },
        Op::Promote {
            region: 1,
            owner: 0,
        },
        Op::Alloc { region: 1, len: 0 },
        Op::Collect { region: 2 },
        Op::GuardEnd,
        Op::GuardEnd,
        Op::Promote {
            region: 1,
            owner: 0,
        }, // handle 3
        Op::Alloc { region: 1, len: 0 }, // RegionLimit at 216/256
        Op::Unroot { handle: 0 },
        Op::Collect { region: 1 },
        Op::Read { handle: 0 },
        Op::Alloc { region: 1, len: 0 }, // handle 4 reuses slot
        Op::Read { handle: 0 },
        Op::Close { region: 1 },
        Op::Create {
            manual: true,
            limit: 256,
        }, // region 3, new ID
        Op::Alloc { region: 3, len: 0 }, // handle 5
        Op::Read { handle: 4 },
        Op::Read { handle: 2 },          // another region survives close
        Op::Alloc { region: 1, len: 0 }, // closed ID stays invalid
        Op::Root { handle: 5 },
        Op::Owned { len: 1 }, // owner 1
        Op::MutateOwned {
            owner: 1,
            byte: 0xa5,
        },
        Op::Promote {
            region: 2,
            owner: 1,
        }, // handle 6
        Op::Copy { handle: 6 }, // owner 2
        Op::MutateOwned {
            owner: 2,
            byte: 0x5a,
        },
        Op::Read { handle: 6 },
        Op::Root { handle: 6 },
        Op::GuardStart { budget: 256 },
        Op::Alloc { region: 2, len: 64 }, // handle 7, reaches trigger
        Op::Collect { region: 2 },
        Op::GuardEnd,
        Op::Alloc { region: 2, len: 0 }, // auto safepoint, handle 8
        Op::Read { handle: 6 },
        Op::Read { handle: 2 },
        Op::Close { region: 3 }, // root blocks close
        Op::Unroot { handle: 5 },
        Op::Close { region: 3 },
        Op::Read { handle: 5 },
        Op::Copy { handle: 6 },
        Op::Unroot { handle: 6 },
        Op::Collect { region: 2 },
        Op::Read { handle: 6 },
    ]
}

/// LCG and upper-32-bit modulo choices match v1-v3, but this trace contract
/// is frozen independently. At most 128 steps, 4 regions, 128 handles, 64-byte
/// payloads, and 2 nested guards.
pub fn generate(seed: u64) -> Vec<Op> {
    let mut rng = Rng(seed);
    let mut ops = prefix();
    let mut depth = 0;
    while ops.len() + depth + 2 <= 128 {
        let pick = rng.choice(19);
        let region = [0, 2, 2, 1, 3][rng.choice(5)];
        let handle = rng.choice(12);
        let len = [0, 1, 7, 31, 64][rng.choice(5)];
        let op = match pick {
            0..=2 => Op::Alloc { region, len },
            3 => Op::Owned { len },
            4 => Op::MutateOwned {
                owner: rng.choice(12),
                byte: rng.next() as u8,
            },
            5 => Op::Promote {
                region,
                owner: rng.choice(12),
            },
            6 => Op::Copy { handle },
            7 => Op::Root { handle },
            8 => Op::Unroot { handle },
            9 | 10 => Op::Link {
                parent: handle,
                child: rng.choice(12),
            },
            11 => Op::Read { handle },
            12 | 13 => Op::Collect { region },
            14 => Op::Close { region },
            15 => Op::ForeignRead,
            16 if depth < 2 => {
                depth += 1;
                Op::GuardStart {
                    budget: [0, 63, 64, 128, 256][rng.choice(5)],
                }
            }
            17 if depth > 0 => {
                depth -= 1;
                Op::GuardEnd
            }
            _ => Op::ForeignRegionAlloc,
        };
        ops.push(op);
    }
    ops.extend(std::iter::repeat_n(Op::GuardEnd, depth));
    ops
}

pub fn fixed_seeds() -> Vec<u64> {
    include_str!("../corpus/seeds-v4.csv")
        .lines()
        .skip(1)
        .map(|line| line.parse().expect("unsigned v4 seed"))
        .collect()
}

const FAMILY_CHECKS: &[(usize, &str, &str)] = &[
    (31, "Err(InvalidHandle)", "stale_after_slot_reuse"),
    (11, "Ok([])", "roots_through_links"),
    (14, "Err(CrossRegionEdge)", "cross_region_link"),
    (25, "Ok", "failed_move_keeps_owner"),
    (44, "Ok([165])", "independent_copy"),
    (26, "Err(RegionLimit)", "zero_length_region_limit"),
    (20, "Err(BudgetExceeded)", "nested_guard_failure"),
    (52, "Err(InvalidHandle)", "auto_after_guard"),
    (15, "Err(InvalidHandle)", "wrong_session_handle"),
    (16, "Err(InvalidRegion)", "wrong_session_region"),
    (36, "Ok(bytes)", "close_recreate_isolation"),
    (53, "Err(RegionHasRoots)", "close_with_roots"),
];

pub fn run_trace(trace: &[Op], fault: Fault) -> Result<Coverage, String> {
    let mut coverage = Coverage::default();
    let mut outcomes = Vec::new();
    v3::run_trace_observed(trace, fault, &mut |op, result| {
        coverage.record(op, result);
        outcomes.push(result.to_owned());
    })?;
    if trace.starts_with(&prefix()) {
        for (index, expected, family) in FAMILY_CHECKS {
            if outcomes.get(*index).is_some_and(|result| {
                result == expected || (*expected == "Ok(bytes)" && result.starts_with("Ok(["))
            }) {
                *coverage.families.entry((*family).into()).or_default() += 1;
            }
        }
    }
    Ok(coverage)
}

pub fn run(seed: u64, fault: Fault) -> Result<Coverage, String> {
    let trace = generate(seed);
    let mut coverage = run_trace(&trace, fault).map_err(|error| {
        format!(
            "model=4 seed={seed}: {error}\noriginal trace:\n{}\nminimized trace:\n{}",
            v3::format_trace(&trace),
            v3::format_trace(&shrink(&trace, fault))
        )
    })?;
    coverage.seeds = 1;
    Ok(coverage)
}

pub fn shrink(trace: &[Op], fault: Fault) -> Vec<Op> {
    v3::shrink(trace, fault)
}
