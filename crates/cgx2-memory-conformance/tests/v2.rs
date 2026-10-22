use cgx2_cli::{check_source, run_checked_source, run_source_with_report, Value};
use cgx2_memory_conformance::{
    fixed_seeds, fixed_seeds_v2, generate, generate_v2, generate_v2_failures,
};
use cgx2_runtime::MemoryReport;

fn counters(report: &MemoryReport) -> String {
    format!(
        "{:?}",
        report
            .regions
            .iter()
            .map(|r| (
                r.is_open,
                r.committed_bytes,
                r.live_bytes,
                r.peak_bytes,
                r.collection_count,
                r.total_collected_bytes,
                r.allocation_failures,
            ))
            .collect::<Vec<_>>()
    )
}

#[test]
fn v2_generation_is_deterministic_and_bounded() {
    let seeds = fixed_seeds_v2();
    assert!(seeds.len() >= 256);
    let mut mode_pairs = std::collections::BTreeSet::new();
    let mut saw_zero = false;
    for seed in seeds {
        let case = generate_v2(seed);
        assert_eq!(case, generate_v2(seed));
        assert_eq!(generate_v2_failures(seed), generate_v2_failures(seed));
        assert!(case.source.len() <= 4096, "seed={seed}");
        assert!(case.steps <= 48, "seed={seed}");
        assert!(case.max_allocation <= 32, "seed={seed}");
        assert!(case.source.contains("memory.promote_owned"));
        assert!(case.source.contains("memory.copy_to_owned"));
        mode_pairs.insert((case.expected.regions[0].mode, case.expected.regions[1].mode));
        saw_zero |= case.source.contains("memory.owned_bytes(0)");
    }
    assert_eq!(mode_pairs.len(), 4);
    assert!(saw_zero);
}

#[test]
fn v2_model_rejects_a_deliberately_mismatched_report() {
    let case = generate_v2(0);
    let (_, report) = run_source_with_report(&case.source, &[]).unwrap();
    let report = report.unwrap();
    case.expected.compare(&report).unwrap();
    let mut wrong = case.expected.clone();
    wrong.regions[0].committed += 1;
    assert!(wrong.compare(&report).is_err());
    let mut wrong_totals = report.clone();
    wrong_totals.totals.total_collected_bytes += 1;
    assert!(case.expected.compare(&wrong_totals).is_err());
}

#[test]
fn both_fixed_corpora_replay_twice_with_identical_sources_and_counters() {
    for (version, seeds) in [(1, fixed_seeds()), (2, fixed_seeds_v2())] {
        for seed in seeds {
            let case = if version == 1 {
                generate(seed)
            } else {
                generate_v2(seed)
            };
            let replay = if version == 1 {
                generate(seed)
            } else {
                generate_v2(seed)
            };
            assert_eq!(case.source.as_bytes(), replay.source.as_bytes());
            assert_eq!(
                format!("{:?}", case.expected),
                format!("{:?}", replay.expected)
            );
            let mut snapshots = Vec::new();
            for _ in 0..2 {
                let (result, report) =
                    run_source_with_report(&case.source, &[]).unwrap_or_else(|e| {
                        panic!("model={version} seed={seed}\n{}\n{e:?}", case.source)
                    });
                assert_eq!(result.value, Value::Unit);
                let report = report.expect("successful source has report");
                case.expected.compare(&report).unwrap_or_else(|e| {
                    panic!("model={version} seed={seed}\n{}\n{e}", case.source)
                });
                snapshots.push(counters(&report));
            }
            assert_eq!(snapshots[0].as_bytes(), snapshots[1].as_bytes());
        }
    }
}

#[test]
fn failed_promotions_return_dynamic_errors_without_success_reports() {
    for seed in fixed_seeds_v2() {
        for fixture in generate_v2_failures(seed) {
            let source = fixture.source;
            assert!(source.len() <= 4096 && source.lines().count() <= 48);
            assert!(check_source(&source).is_ok(), "seed={seed}\n{source}");
            let error = run_source_with_report(&source, &[]).unwrap_err();
            assert_eq!(error.code, fixture.expected_code, "seed={seed}\n{source}");
            assert!(
                error.message.contains(fixture.expected_error),
                "seed={seed} {error:?}"
            );
            let span = error.span.expect("source span");
            assert!(source[span.start..span.end].contains(
                if source.contains("memory.copy_to_owned") {
                    "memory.copy_to_owned"
                } else {
                    "memory.promote_owned"
                }
            ));
        }
    }
}

#[test]
fn invalid_source_operations_have_exact_diagnostic_families_and_spans() {
    let fixtures = [
        ("let r = memory.region(mode: manual, limit: 256)\nlet o = memory.owned_bytes(1)\nlet m = memory.promote_owned(r, o)\nmemory.use(o)\n", "CGO001", "memory.use(o)"),
        ("let o = memory.owned_bytes(1)\nlet b = memory.borrow(o)\nlet c = memory.borrow_mut(o)\n", "CGO002", "memory.borrow_mut(o)"),
        ("let r = memory.region(mode: manual, limit: 256)\nlet x = memory.alloc_bytes(r, 1)\nmemory.close(r)\n", "CGO003", "memory.close(r)"),
        ("let r = memory.region(mode: manual, limit: 256)\nno_gc(budget: 128):\n    memory.collect(r)\n", "CGO004", "memory.collect(r)"),
        ("let r = memory.region(mode: manual, limit: 256)\nlet x = memory.alloc_bytes(r, 1)\nmemory.drop(x)\nmemory.collect(r)\nmemory.use(x)\n", "CGO001", "memory.use(x)"),
    ];
    for (source, code, operation) in fixtures {
        let errors = run_checked_source(source, &[]).unwrap_err();
        let match_error = errors
            .iter()
            .find(|e| e.code == code)
            .unwrap_or_else(|| panic!("{source}\n{errors:?}"));
        let span = match_error.span.expect("source span");
        assert!(
            source[span.start..span.end].contains(operation),
            "{source}\n{match_error:?}"
        );
    }
}
