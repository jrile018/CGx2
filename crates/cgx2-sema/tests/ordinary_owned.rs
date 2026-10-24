use cgx2_ownership::v1::{self as o, ResultMode};
use cgx2_sema::ordinary_owned::{compile_source, Scalar};
const SOURCE: &str =
    include_str!("../../cgx2-syntax/ordinary-owned-evidence/fixtures/functions.cg");
const FAILURE: &str =
    include_str!("../../cgx2-syntax/ordinary-owned-evidence/fixtures/arithmetic-failure.cg");

#[test]
fn source_storage_and_mixed_parameter_provenance() {
    let program = compile_source(SOURCE).unwrap();
    assert_eq!(program.source_sha256().len(), 64);
    let model = program.model();
    assert_eq!(model.layouts[0].size, 9);
    assert_eq!(
        model.layouts[0]
            .fields
            .iter()
            .map(|f| f.offset)
            .collect::<Vec<_>>(),
        [0, 8]
    );
    for name in ["identity", "read", "lend", "update"] {
        let f = model.functions.iter().find(|f| f.name == name).unwrap();
        assert_eq!(f.ownership_parameters, [None, Some(0), None]);
    }
    assert_eq!(
        program
            .summaries()
            .iter()
            .find(|s| s.name == "identity")
            .unwrap()
            .result,
        ResultMode::Owned {
            from_parameter: Some(0)
        }
    );
    assert_eq!(
        program
            .summaries()
            .iter()
            .find(|s| s.name == "lend")
            .unwrap()
            .result,
        ResultMode::Loan {
            parameter: 0,
            projection: vec![],
            kind: o::LoanKind::Shared
        }
    );
    assert_eq!(
        program
            .summaries()
            .iter()
            .find(|s| s.name == "read")
            .unwrap()
            .result,
        ResultMode::Unit
    );
    for f in program.ownership_functions() {
        let other = program
            .summaries()
            .iter()
            .filter(|s| s.name != f.summary.name)
            .cloned()
            .collect::<Vec<_>>();
        assert!(o::analyze_function(f, &other, SOURCE, o::AnalysisLimits::default()).is_accepted());
    }
    let result = program.execute().unwrap();
    assert_eq!(result.result, Ok(Scalar::I64(37)));
    assert_eq!(result.evidence.allocations, 4);
    assert_eq!(result.evidence.releases, 4);
    assert_eq!(result.evidence.live_bytes, 0);
    assert_eq!(result.evidence.allocated_bytes, 36);
    assert_eq!(result.evidence.released_bytes, 36);
    assert_eq!(result.evidence.peak_live_bytes, 18);
    assert_eq!(result.evidence.reads, 14);
    assert_eq!(result.evidence.writes, 17);
}

#[test]
fn nonzero_ownership_parameter_results_remap_across_nested_calls() {
    let source = include_str!("../../cgx2-syntax/ordinary-owned-evidence/fixtures/mapping.cg");
    let p = compile_source(source).unwrap();
    let f = p
        .model()
        .functions
        .iter()
        .find(|f| f.name == "choose")
        .unwrap();
    assert_eq!(f.ownership_parameters, [None, Some(0), None, Some(1), None]);
    assert_eq!(
        p.summaries()
            .iter()
            .find(|f| f.name == "choose")
            .unwrap()
            .result,
        ResultMode::Owned {
            from_parameter: Some(1)
        }
    );
    for name in ["lend", "relay"] {
        assert_eq!(
            p.summaries()
                .iter()
                .find(|f| f.name == name)
                .unwrap()
                .result,
            ResultMode::Loan {
                parameter: 1,
                projection: vec![],
                kind: o::LoanKind::Shared
            }
        );
    }
    let e = p.execute().unwrap();
    assert_eq!(e.result, Ok(Scalar::I64(22)));
    assert_eq!(e.evidence.allocations, e.evidence.releases);
}
#[test]
fn checked_arithmetic_releases_callee_and_caller_storage() {
    let result = compile_source(FAILURE).unwrap().execute().unwrap();
    let error = result.result.unwrap_err();
    assert_eq!(error.code, "CGR080");
    assert_eq!(&FAILURE[error.span.start..error.span.end], "p.x / n");
    assert_eq!(result.evidence.allocations, 2);
    assert_eq!(result.evidence.releases, 2);
    assert_eq!(result.evidence.live_bytes, 0);
}
#[test]
fn repeated_calls_release_each_owner_instead_of_retaining_until_program_exit() {
    let prefix = "record R:\n    x: i64\nfn once(n: i64) -> i64:\n    let p = owned(R(x: n))\n    p.x\nfn main() -> i64:\n";
    let mut source = prefix.to_owned();
    for i in 0..100 {
        source.push_str(&format!("    let n{i} = once({i})\n"));
    }
    source.push_str("    n99\n");
    let result = compile_source(&source).unwrap().execute().unwrap();
    assert_eq!(result.result, Ok(Scalar::I64(99)));
    assert_eq!(result.evidence.allocations, 100);
    assert_eq!(result.evidence.releases, 100);
    assert_eq!(result.evidence.peak_live_bytes, 8);
}
#[test]
fn exclusive_result_and_branch_cleanup_are_source_checked() {
    let source = "record R:\n    x: i64\nfn lend(n: i64, p: exclusive<R>) -> exclusive<R>:\n    p\nfn main() -> i64:\n    let p = owned(R(x: 1))\n    if true:\n        let w = lend(0, borrow_mut(p))\n        w.x = 42\n    else:\n        0\n    p.x\n";
    assert_eq!(
        compile_source(source).unwrap().execute().unwrap().result,
        Ok(Scalar::I64(42))
    );
}
#[test]
fn foundation_spelling_does_not_select_intrinsics_after_ordinary_resolution() {
    let source = "record owned:\n    x: i64\nfn borrow(x: owned) -> i64:\n    x.x\nfn main() -> i64:\n    borrow(owned(x: 3))\n";
    let m = cgx2_syntax::parse_module(source).unwrap();
    assert!(cgx2_syntax::ordinary_owned_span(&m).is_none());
    assert!(cgx2_sema::check_module(&m).is_ok());
}
#[test]
fn copied_consumers_cannot_reinterpret_an_owned_program() {
    let m = cgx2_syntax::parse_module(SOURCE).unwrap();
    assert_eq!(cgx2_sema::check_module(&m).unwrap_err()[0].code, "CGT080");
    assert_eq!(
        cgx2_sema::check_module_with_imports(&m, &Default::default()).unwrap_err()[0].code,
        "CGT080"
    );
}

#[test]
fn recursive_sccs_and_default_v1_resource_exhaustion_never_publish_summaries() {
    let source = "record R:\n    x: i64\nfn recurse(p: owned<R>) -> owned<R>:\n    recurse(p)\nfn main() -> i64:\n    0\n";
    let ds = compile_source(source).unwrap_err();
    assert_eq!(ds[0].code, "CGT081");
    assert!(ds[0].message.contains("recursive ownership SCC"));
    let mut source = "record R:\n    x: i64\nfn main() -> i64:\n".to_owned();
    for i in 0..130 {
        source.push_str(&format!("    let p{i} = owned(R(x: {i}))\n"));
    }
    source.push_str("    0\n");
    assert_eq!(compile_source(&source).unwrap_err()[0].code, "CGO106");
}

#[test]
fn exact_source_diagnostics_for_moves_loans_layouts_and_provenance() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../cgx2-syntax/ordinary-owned-evidence/fixtures");
    let cases = [
        ("use-after-move", "CGO101", "p.x"),
        ("argument-move-order", "CGO101", "p.x"),
        ("conflicting-loans", "CGO102", "borrow_mut(p)"),
        ("escaped-local", "CGO103", "borrow(p)"),
        ("invalid-field", "CGT081", "p.missing"),
        ("invalid-layout", "CGT081", "String"),
        ("invalid-parameter", "CGT081", "borrow(p)"),
        ("unsupported-reborrow", "CGT083", "q"),
        ("reserved-shadow", "CGT082", "let borrow = 1\n"),
        ("owner-result-as-loan", "CGO103", "borrow(p)"),
    ];
    for (fixture, code, snippet) in cases {
        let source = std::fs::read_to_string(directory.join(format!("{fixture}.cg"))).unwrap();
        let ds = compile_source(&source).unwrap_err();
        assert_eq!(ds[0].code, code, "{fixture}: {ds:?}");
        assert_eq!(
            &source[ds[0].span.start..ds[0].span.end],
            snippet,
            "{fixture}"
        );
    }
}

#[cfg(feature = "failure-injection-test")]
#[test]
fn allocation_faults_are_source_located_and_release_prior_source_owners() {
    let program = compile_source(FAILURE).unwrap();
    let mut owned_failure = false;
    for index in 1..=16 {
        if let Ok(session) = cgx2_runtime::Session::try_new_with_host_failure_at_reserve(index) {
            let result = program.execute_with_session(session).unwrap();
            assert_eq!(result.evidence.allocations, result.evidence.releases);
            assert_eq!(result.evidence.live_bytes, 0);
            if let Err(failure) = result.result {
                if failure.message.contains("owned allocation failed") {
                    owned_failure = true;
                    assert!(FAILURE[failure.span.start..failure.span.end].starts_with("owned("));
                }
            }
        }
    }
    assert!(owned_failure);
}
