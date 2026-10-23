use cgx2_cli::{run_checked_source, run_checked_source_with_failure, Value};

const SUCCESS: &str = include_str!("../../cgx2c/tests/fixtures/conditional_memory_success.cg");
const FUNCTIONS: &str = include_str!("../../cgx2c/tests/fixtures/conditional_memory_functions.cg");

#[test]
fn chosen_allocations_names_and_post_join_root_release() {
    for source in [SUCCESS, FUNCTIONS] {
        let (result, Some(report)) = run_checked_source(source, &[]).unwrap() else {
            panic!()
        };
        assert_eq!(result.value, Value::Unit);
        assert_eq!(report.totals.collection_count, 0);
        assert_eq!(report.totals.allocation_failures, 0);
        assert_eq!(report.totals.live_bytes, 0);
        assert_eq!(report.totals.committed_bytes, 0);
        assert!(report.regions.iter().skip(1).all(|region| !region.is_open));
    }
    let failure = run_checked_source_with_failure(&SUCCESS.replace("if true:", "if false:"), &[])
        .err()
        .unwrap()
        .remove(0);
    assert!(failure.error.message.contains("RegionLimit"));
}

#[test]
fn all_comparison_guards_choose_once_using_i64_parameters() {
    for (op, a, b, selected) in [
        ("==", 1, 1, true),
        ("!=", 1, 1, false),
        ("<", 1, 2, true),
        ("<=", 2, 1, false),
        (">", 2, 1, true),
        (">=", 1, 2, false),
    ] {
        for (a, b, selected) in [
            (a, b, selected),
            (
                b,
                a,
                if op == "==" || op == "!=" {
                    selected
                } else {
                    !selected
                },
            ),
        ] {
            let source = format!("fn choose(a: i64, b: i64) -> Unit effects(ManagedAlloc, Mutate):\n    no_gc(budget: 40):\n        if a {op} b:\n            let temp = memory.owned_bytes({})\n            memory.drop(temp)\n        else:\n            let temp = memory.owned_bytes({})\n            memory.drop(temp)\nfn main() -> Unit effects(ManagedAlloc, Mutate):\n    choose({a}, {b})\n", if selected { 8 } else { 9 }, if selected { 9 } else { 8 });
            let (_, Some(report)) = run_checked_source(&source, &[]).unwrap() else {
                panic!()
            };
            assert_eq!(report.totals.collection_count, 0);
            assert_eq!(report.totals.live_bytes, 0);
        }
    }
}

#[test]
fn nested_failure_census_precedes_local_and_function_cleanup() {
    for (source, detail, roots) in [
        (
            include_str!("../../cgx2c/tests/fixtures/conditional_memory_region_failure.cg"),
            "RegionLimit",
            2,
        ),
        (
            include_str!("../../cgx2c/tests/fixtures/conditional_memory_budget_failure.cg"),
            "BudgetExceeded",
            1,
        ),
        (
            include_str!("../../cgx2c/tests/fixtures/conditional_memory_negative.cg"),
            "negative",
            1,
        ),
    ] {
        let failure = run_checked_source_with_failure(source, &[])
            .err()
            .unwrap()
            .remove(0);
        assert_eq!(failure.error.code, "CGR013");
        assert!(
            failure.error.message.contains(detail),
            "{}",
            failure.error.message
        );
        assert_eq!(failure.rollback_preserved, Some(true));
        assert!(failure.cleanup_errors.is_empty());
        let operation = failure.operation_failure.unwrap();
        assert_eq!(operation.rooted_handles, Some(roots));
        assert_eq!(operation.runtime_root_registrations, Some(roots));
        let final_state = failure.final_unwind.unwrap();
        assert_eq!(final_state.rooted_handles, Some(0));
        assert_eq!(final_state.runtime_root_registrations, Some(0));
        assert_eq!(final_state.memory.totals.collection_count, 0);
        assert_eq!(
            final_state.memory.totals.live_bytes,
            operation.memory.totals.live_bytes
        );
    }
}
