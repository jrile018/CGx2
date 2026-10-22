use cgx2_cli::{run_checked_source, run_source_with_report, Value};
use cgx2_memory_conformance::{fixed_seeds, generate, MODEL_VERSION};
use cgx2_runtime::{AllocationError, CollectionMode, Session, SLOT_METADATA_BYTES};

#[test]
fn fixed_corpus_is_deterministic_bounded_and_matches_runtime() {
    let seeds = fixed_seeds();
    assert!(seeds.len() >= 100);
    for seed in seeds {
        let case = generate(seed);
        let again = generate(seed);
        assert_eq!(case, again, "seed={seed}");
        assert!(case.source.lines().count() <= 48, "seed={seed}");
        assert!(case.source.len() <= 4096, "seed={seed}");
        assert!(case.steps <= 48 && case.regions == 2 && case.max_allocation <= 32);
        let outcome = run_source_with_report(&case.source, &[]);
        let (result, report) = outcome.unwrap_or_else(|actual| {
            panic!("seed={seed} model={MODEL_VERSION}\nsource:\n{}\nexpected={:#?}\nactual={actual:#?}\nreport=<unavailable on error>", case.source, case.expected)
        });
        assert_eq!(result.value, Value::Unit, "seed={seed}\n{}", case.source);
        let actual = report.expect("memory report");
        if let Err(reason) = case.expected.compare(&actual) {
            panic!("seed={seed} model={MODEL_VERSION}\nsource:\n{}\nexpected={:#?}\nactual={reason}\nreport={:#?}", case.source, case.expected, actual);
        }
    }
}

#[test]
fn one_operation_mutations_are_rejected_with_exact_diagnostics() {
    let valid = "let r = memory.region(mode: manual, limit: 4096)\nlet o = memory.owned_bytes(8)\nlet m = memory.move(o)\nmemory.use(m)\nlet b = memory.borrow(m)\nmemory.end_borrow(b)\nlet x = memory.alloc_bytes(r, 8)\nmemory.drop(x)\nno_gc(budget: 128):\n    let y = memory.alloc_bytes(r, 8)\nmemory.drop(y)\nmemory.close(r)\n";
    assert!(run_checked_source(valid, &[]).is_ok());
    let fixtures: [(String, &[&str]); 4] = [
        (
            valid.replacen("memory.use(m)", "memory.use(o)", 1),
            &["CGO001"],
        ),
        (
            valid.replacen("memory.end_borrow(b)", "let c = memory.borrow_mut(m)", 1),
            &["CGO002", "CGO002"],
        ),
        (
            valid.replacen("memory.drop(y)", "memory.use(y)", 1),
            &["CGO003"],
        ),
        (
            valid.replacen(
                "    let y = memory.alloc_bytes(r, 8)",
                "    memory.collect(r)\n    let y = memory.alloc_bytes(r, 8)",
                1,
            ),
            &["CGO004"],
        ),
    ];
    for (source, expected_codes) in fixtures {
        let errors = run_checked_source(&source, &[]).unwrap_err();
        let codes: Vec<_> = errors.iter().map(|error| error.code).collect();
        assert_eq!(
            codes, expected_codes,
            "source:\n{source}\nerrors={errors:#?}"
        );
        assert!(errors.iter().all(|error| error.span.is_some()));
    }
}

#[test]
fn nested_budget_failure_preserves_prior_allocation_without_collection() {
    let source = "let a = memory.region(mode: auto, limit: 4096)\nlet b = memory.region(mode: manual, limit: 4096)\nno_gc(budget: 144):\n    let x = memory.alloc_bytes(a, 8)\n    no_gc(budget: 71):\n        let y = memory.alloc_bytes(b, 8)\n";
    let error = run_source_with_report(source, &[]).unwrap_err();
    assert_eq!(error.code, "CGR013");
    assert!(error.message.contains("BudgetExceeded"));
    assert!(error.span.is_some());

    // The source API returns no report on error; this companion checks runtime state.
    let mut session = Session::new();
    let a = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: 1 << 20,
            },
            4096,
        )
        .unwrap();
    let b = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let before = session.memory_report();
    {
        let mut outer = session.no_gc(144);
        let first = outer.alloc_bytes(a, 8).unwrap();
        outer.root(first).unwrap();
        {
            let mut inner = outer.no_gc(71);
            assert_eq!(
                inner.alloc_bytes(b, 8),
                Err(AllocationError::BudgetExceeded)
            );
            assert_eq!(
                inner.memory_report().totals.collection_count,
                before.totals.collection_count
            );
        }
    }
    let after = session.memory_report();
    assert_eq!(
        after.totals.collection_count,
        before.totals.collection_count
    );
    assert_eq!(after.regions[1].committed_bytes, SLOT_METADATA_BYTES + 8);
    assert_eq!(after.regions[1].live_bytes, SLOT_METADATA_BYTES + 8);
    assert_eq!(after.regions[2].committed_bytes, 0);
    assert_eq!(after.regions[2].allocation_failures, 1);
}

#[test]
fn region_limit_is_dynamic_and_returns_no_success_report() {
    let source =
        "let r = memory.region(mode: manual, limit: 64)\nlet x = memory.alloc_bytes(r, 1)\n";
    assert!(cgx2_cli::check_source(source).is_ok());
    let error = run_source_with_report(source, &[]).unwrap_err();
    assert_eq!(error.code, "CGR013");
    assert!(error.message.contains("RegionLimit"));
    assert!(error.span.is_some());
}
