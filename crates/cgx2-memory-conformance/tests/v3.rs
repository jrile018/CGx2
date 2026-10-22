use cgx2_memory_conformance::v3::{generate, run, shrink, Fault, Op, MODEL_VERSION_V3};

#[test]
fn deterministic_and_bounded() {
    let a = generate(42);
    assert_eq!(a, generate(42));
    assert_eq!(MODEL_VERSION_V3, 3);
    assert!(a.len() <= 128);
    assert!(a
        .iter()
        .all(|op| op.payload_len().is_none_or(|len| len <= 32)));
    run(42, Fault::None).unwrap();
}

#[test]
fn faulty_model_is_replayable_and_shrinks() {
    let trace = vec![Op::Alloc { region: 0, len: 1 }, Op::Read { handle: 0 }];
    let first = run_trace(&trace);
    assert_eq!(first, run_trace(&trace));
    let reduced = shrink(&trace, Fault::OvercountFirstAllocation);
    assert!(reduced.len() < trace.len());
    assert!(run_trace(&reduced).is_err());
}

#[test]
fn auto_safepoint_and_region_isolation_follow_model() {
    let trace = vec![
        Op::Create {
            manual: false,
            limit: 512,
        },
        Op::Create {
            manual: true,
            limit: 256,
        },
        Op::Alloc { region: 1, len: 32 },
        Op::Alloc { region: 1, len: 32 },
        Op::Alloc { region: 1, len: 32 },
        Op::Alloc { region: 2, len: 0 },
        Op::Root { handle: 3 },
        Op::Alloc { region: 1, len: 32 }, // auto collection reclaims first three
        Op::Read { handle: 0 },
        Op::Read { handle: 3 },
        Op::Collect { region: 1 },
        Op::Read { handle: 3 },
    ];
    cgx2_memory_conformance::v3::run_trace(&trace, Fault::None).unwrap();
}

fn run_trace(trace: &[Op]) -> Result<(), String> {
    cgx2_memory_conformance::v3::run_trace(trace, Fault::OvercountFirstAllocation)
}

#[test]
fn fixed_manifest_passes() {
    let seeds = include_str!("../corpus/seeds-v3.csv");
    assert_eq!(seeds.lines().count(), 1001);
    let mut first_pass = Vec::new();
    for pass in 1..=2 {
        for (expected_seed, raw) in seeds.lines().skip(1).enumerate() {
            let seed: u64 = raw.parse().unwrap();
            assert_eq!(seed, expected_seed as u64);
            let trace = generate(seed);
            assert!(trace.len() <= 96, "seed={seed}");
            if pass == 1 {
                first_pass.push(trace);
            } else {
                assert_eq!(trace, first_pass[expected_seed], "seed={seed}");
            }
            run(seed, Fault::None)
                .unwrap_or_else(|error| panic!("pass={pass} seed={seed}: {error}"));
        }
    }
}
