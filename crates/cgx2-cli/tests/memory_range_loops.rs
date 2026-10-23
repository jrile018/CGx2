use cgx2_cli::{run_checked_source, run_checked_source_with_failure};

fn success(source: &str) {
    let (_, Some(report)) = run_checked_source(source, &[]).unwrap() else {
        panic!()
    };
    assert_eq!(report.totals.live_bytes, 0);
    assert_eq!(report.totals.allocation_failures, 0);
}

#[test]
fn signed_boundaries_zero_one_many_and_fresh_sibling_scopes() {
    for (start, end, budget) in [
        (0, 0, 0),
        (2, 1, 0),
        (0, 1, 33),
        (-2, 2, 132),
        (i64::MIN, i64::MIN + 1, 33),
        (i64::MAX - 1, i64::MAX, 33),
        (i64::MAX, i64::MIN, 0),
    ] {
        success(&format!("no_gc(budget: {budget}):\n    for i in range({start}, {end}):\n        let t = memory.owned_bytes(1)\n        memory.drop(t)\nfor i in range(0, 3):\n    let t = memory.owned_bytes(i)\n    memory.drop(t)\n"));
    }
}

#[test]
fn exact_threshold_is_shared_through_nesting_calls_and_branches() {
    let source = "fn leaf(n: i64) -> Unit effects(ManagedAlloc, Mutate):\n    let marker = memory.owned_bytes(0)\n    for j in range(0, n):\n        memory.use(marker)\n    memory.drop(marker)\nfn main() -> Unit effects(ManagedAlloc, Mutate):\n    for i in range(0, 10):\n        leaf(9999)\n";
    success(source); // 10 + 10 * 9999 = 100000, below the statement-step limit.
    let error = run_checked_source_with_failure(&source.replace("9999", "10000"), &[])
        .err()
        .unwrap()
        .remove(0);
    assert_eq!(error.error.code, "CGR026");
    let span = error.error.span.unwrap();
    assert!(source[span.start..span.end].starts_with("for j"));
    assert_eq!(error.rollback_preserved, Some(true));
    assert_eq!(
        error.final_unwind.unwrap().runtime_root_registrations,
        Some(0)
    );
    success("let marker = memory.owned_bytes(0)\nfor i in range(0, 100000):\n    memory.use(marker)\nmemory.drop(marker)\n");
    let error = run_checked_source_with_failure("let marker = memory.owned_bytes(0)\nfor i in range(-9223372036854775808, 9223372036854775807):\n    memory.use(marker)\nmemory.drop(marker)\n", &[]).err().unwrap().remove(0);
    assert_eq!(error.error.code, "CGR026");
}

#[test]
fn cumulative_budget_and_repeated_manual_pressure_have_precleanup_census() {
    let source = include_str!("../../cgx2c/tests/fixtures/range_memory_budget_failure.cg");
    success(&source.replace("budget: 711", "budget: 712"));
    let error = run_checked_source_with_failure(source, &[])
        .err()
        .unwrap()
        .remove(0);
    assert!(error.error.message.contains("BudgetExceeded"));
    assert_eq!(error.rollback_preserved, Some(true));
    assert!(error.cleanup_errors.is_empty());
    let before = error.operation_failure.unwrap();
    let after = error.final_unwind.unwrap();
    assert_eq!(before.runtime_root_registrations, Some(2));
    assert_eq!(after.runtime_root_registrations, Some(0));
    assert_eq!(before.memory.totals.collection_count, 0);
    let pressure = "let r = memory.region(mode: manual, limit: 100)\nfor i in range(0, 4):\n    let t = memory.alloc_bytes(r, 1)\n    memory.drop(t)\nmemory.close(r)\n";
    assert!(run_checked_source_with_failure(pressure, &[])
        .err()
        .unwrap()[0]
        .error
        .message
        .contains("RegionLimit"));
    success(&pressure.replace(
        "    memory.drop(t)",
        "    memory.drop(t)\n    memory.collect(r)",
    ));
}

#[cfg(feature = "failure-injection-test")]
#[test]
fn reserve_faults_keep_first_error_and_unwind_every_iteration_scope() {
    let source = include_str!("../../cgx2c/tests/fixtures/range_memory_budget_failure.cg")
        .replace("budget: 711", "budget: 712");
    let mut saw_local_roots = false;
    for reserve in 1..=24 {
        let result = cgx2_cli::run_checked_source_with_failure_at_reserve(&source, &[], reserve);
        if let Err(errors) = result {
            let failure = &errors[0];
            assert!(
                failure.error.message.contains("HostAllocation"),
                "reserve={reserve}: {}",
                failure.error.message
            );
            assert!(failure.cleanup_errors.is_empty());
            let roots = failure
                .operation_failure
                .as_ref()
                .unwrap()
                .runtime_root_registrations
                .unwrap();
            println!("reserve={reserve} roots={roots}");
            saw_local_roots |= roots == 2;
            assert_eq!(
                failure
                    .final_unwind
                    .as_ref()
                    .unwrap()
                    .runtime_root_registrations,
                Some(0)
            );
        }
    }
    assert!(saw_local_roots);
}

#[test]
fn statement_steps_remain_a_separate_limit_with_first_failure_snapshot() {
    let source = include_str!("../../cgx2c/tests/fixtures/range_memory_steps.cg");
    let error = run_checked_source_with_failure(source, &[])
        .err()
        .unwrap()
        .remove(0);
    assert_eq!(error.error.code, "CGR010");
    assert_eq!(error.rollback_preserved, Some(true));
    assert_eq!(
        error.operation_failure.unwrap().runtime_root_registrations,
        Some(1)
    );
    assert_eq!(
        error.final_unwind.unwrap().runtime_root_registrations,
        Some(0)
    );
}

#[test]
fn lexical_nested_indices_conditionals_and_no_gc_guards_restore_scopes() {
    let source = include_str!("../../cgx2c/tests/fixtures/range_memory_success.cg");
    success(source);
    let failure =
        run_checked_source_with_failure(&source.replace("budget: 198", "budget: 197"), &[])
            .err()
            .unwrap()
            .remove(0);
    assert!(failure.error.message.contains("BudgetExceeded"));
    assert_eq!(
        failure.final_unwind.unwrap().runtime_root_registrations,
        Some(0)
    );
}
