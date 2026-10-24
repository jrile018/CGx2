use cgx2_sema::ordinary_owned::{compile_source_v2 as compile_source, Scalar};

const R: &str = "record R:\n    x: i64\n";
fn execute(body: &str) -> cgx2_sema::ordinary_owned::ExecutionV2 {
    let source = format!("{R}{body}");
    compile_source(&source)
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
        .execute()
        .unwrap()
}
fn clean(e: &cgx2_sema::ordinary_owned::ExecutionV2) {
    assert_eq!(e.evidence.allocations, e.evidence.releases);
    assert_eq!(e.evidence.live_bytes, 0);
    assert_eq!(e.evidence.collection_count, 0);
    assert_eq!(e.evidence.active_guards_at_exit, 0);
    assert_eq!(e.evidence.guard_entries, e.evidence.guard_exits);
}
#[test]
fn actual_source_sum_and_recovery() {
    for source in [
        include_str!("../../cgx2-syntax/owned-result-evidence/fixtures/success.cg"),
        include_str!("../../cgx2-syntax/owned-result-evidence/fixtures/recover.cg"),
    ] {
        let p = compile_source(source).unwrap();
        assert_eq!(p.schema_version(), 2);
        assert!(p.ownership_functions().is_empty());
        assert!(!p.ownership_functions_v2().is_empty());
        let e = p.execute().unwrap();
        assert_eq!(e.result, Ok(Scalar::I64(42)));
        clean(&e);
    }
}
#[test]
fn actual_owned_charge_boundaries_and_nonrefund() {
    for (budget, ok, err) in [(0, 0, 2), (39, 0, 2), (40, 1, 1), (79, 1, 1), (80, 2, 0)] {
        let e = execute(&format!("fn main() -> i64:\n    no_gc(budget: {budget}):\n        try_owned(R(x: 1))\n        try_owned(R(x: 2))\n    42\n"));
        assert_eq!(e.result, Ok(Scalar::I64(42)));
        assert_eq!((e.evidence.result_ok, e.evidence.result_err), (ok, err));
        assert_eq!(e.evidence.owned_charged_bytes, 40 * ok as u128);
        assert_eq!(e.evidence.guard_charged_bytes, 16);
        assert_eq!(e.evidence.peak_live_bytes, if ok == 0 { 0 } else { 8 });
        clean(&e);
    }
}
#[test]
fn nested_guard_table_and_allocation_boundaries_are_separate() {
    for budget in [0, 31, 32] {
        let source = format!("fn main() -> i64:\n    no_gc(budget: {budget}):\n        no_gc(budget: 0):\n            0\n    42\n");
        let e = execute(&source);
        if budget < 32 {
            let error = e.result.as_ref().unwrap_err();
            assert!(error.message.contains("no_gc entry failed: BudgetExceeded"));
            assert_eq!(
                error.span.start,
                R.len() + source.find("no_gc(budget: 0):\n            0").unwrap()
            );
            assert_eq!(e.evidence.guard_failures, 1);
            assert_eq!(e.evidence.guard_entries, 1);
        } else {
            assert_eq!(e.result, Ok(Scalar::I64(42)));
            assert_eq!(e.evidence.guard_charged_bytes, 48);
        }
        assert_eq!(e.evidence.allocations, 0);
        clean(&e);
    }
    for outer in [71, 72] {
        let e = execute(&format!("fn alloc() -> Unit:\n    try_owned(R(x: 1))\n    return\nfn main() -> i64:\n    no_gc(budget: {outer}):\n        no_gc(budget: 40):\n            alloc()\n    42\n"));
        assert_eq!(e.result, Ok(Scalar::I64(42)));
        assert_eq!(e.evidence.allocations, usize::from(outer == 72));
        clean(&e);
    }
}
#[test]
fn mixed_copied_owned_loan_and_result_parameter_remapping() {
    let source = format!("{R}fn pass(a: Bool, old: owned<R>, n: i64, view: shared<R>, q: Result<owned<R>, AllocationError>) -> Result<owned<R>, AllocationError>:\n    let snapshot = view.x\n    q\nfn main() -> i64:\n    let owner = owned(R(x: 9))\n    let old = owned(R(x: 4))\n    let q = pass(true, old, 1, borrow(owner), try_owned(R(x: 42)))\n    match q:\n        Result.Ok(p):\n            return p.x\n        Result.Err(e):\n            return 0\n");
    let p = compile_source(&source).unwrap();
    assert_eq!(
        p.model().functions[0].ownership_parameters,
        [None, Some(0), None, Some(1), Some(2)]
    );
    assert_eq!(
        p.summaries_v2()
            .iter()
            .find(|s| s.name == "pass")
            .unwrap()
            .result,
        cgx2_ownership::v2::ResultMode::OwnedResult {
            from_parameter: Some(2)
        }
    );
    let e = p.execute().unwrap();
    assert_eq!(e.result, Ok(Scalar::I64(42)));
    clean(&e);
}
#[test]
fn prior_owner_survives_later_failure_and_failed_argument_cleans_sum() {
    let e = execute("fn main() -> i64:\n    let prior = owned(R(x: 42))\n    no_gc(budget: 0):\n        match try_owned(R(x: 3)):\n            Result.Ok(p):\n                return p.x\n            Result.Err(e):\n                return prior.x\n");
    assert_eq!(e.result, Ok(Scalar::I64(42)));
    clean(&e);
    let e = execute("fn consume(q: Result<owned<R>, AllocationError>, n: i64) -> Unit:\n    return\nfn main() -> i64:\n    no_gc(budget: 40):\n        consume(try_owned(R(x: 42)), 1 / 0)\n    0\n");
    assert_eq!(e.result.unwrap_err().code, "CGR080");
    assert_eq!(e.evidence.releases, 1);
    assert_eq!(e.evidence.guard_entries, e.evidence.guard_exits);
}
#[test]
fn sum_move_and_arm_scope_negatives_keep_utf8_spans() {
    for body in [
        "let a = try_owned(R(x: 1))\n    let b = a\n    let c = a\n    0",
        "let a = try_owned(R(x: 1))\n    match a:\n        Result.Ok(p):\n            let moved = p\n            let twice = p\n        Result.Err(e):\n            0\n    0",
        "let a = try_owned(R(x: 1))\n    match a:\n        Result.Ok(p):\n            0\n        Result.Err(e):\n            let invalid = p\n    0",
        "let a = try_owned(R(x: 1))\n    match a:\n        Result.Ok(p):\n            0\n        Result.Err(e):\n            0\n    let invalid = a\n    0",
    ] {
        let source = format!("# π\n{R}fn main() -> i64:\n    {body}\n");
        let error = compile_source(&source).unwrap_err().remove(0);
        assert!(matches!(error.code, "CGO101" | "CGT081"));
        assert!(source.is_char_boundary(error.span.start));
        assert!(source.is_char_boundary(error.span.end));
    }
}

#[test]
fn additive_source_bound_does_not_narrow_schema_one_copied_bodies() {
    let source = format!(
        "{R}fn main() -> i64:\n    let p = owned(R(x: 1))\n    let mut n = 0\n{}    n\n",
        "    n += 1\n".repeat(4097)
    );
    let p = compile_source(&source).unwrap();
    assert_eq!(p.schema_version(), 1);
    assert_eq!(p.execute().unwrap().result, Ok(Scalar::I64(4097)));
    let extended = source.replace("owned(R(x: 1))", "try_owned(R(x: 1))");
    let errors = compile_source(&extended).unwrap_err();
    assert_eq!(errors[0].code, "CGT081");
    assert!(errors[0].message.contains("limit is 4096"));
}
#[cfg(feature = "failure-injection-test")]
#[test]
fn injected_host_failures_are_typed_and_cleanup_is_real() {
    let source = format!("{R}fn main() -> i64:\n    let prior = owned(R(x: 42))\n    no_gc(budget: 40):\n        match try_owned(R(x: 3)):\n            Result.Ok(p):\n                return p.x\n            Result.Err(e):\n                match e:\n                    AllocationError.BudgetExceeded():\n                        return 1\n                    AllocationError.RegionLimit():\n                        return 2\n                    AllocationError.HostAllocation():\n                        return prior.x\n                    AllocationError.InvalidRegion():\n                        return 4\n                    AllocationError.ShapeLimit():\n                        return 5\n");
    let p = compile_source(&source).unwrap();
    // SessionRegions=1, earlier OwnedBytes=2, GuardBudgets=3, later OwnedBytes=4.
    for index in [2, 3, 4] {
        let session = cgx2_runtime::Session::try_new_with_host_failure_at_reserve(index).unwrap();
        let e = p.execute_with_session(session).unwrap();
        if index == 4 {
            assert_eq!(e.result, Ok(Scalar::I64(42)));
            assert_eq!(e.evidence.result_err, 1);
        } else {
            assert!(e
                .result
                .as_ref()
                .unwrap_err()
                .message
                .contains("HostAllocation"));
        }
        clean(&e);
    }
}
