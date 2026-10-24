use cgx2_runtime::{default_managed::Snapshot, ordinary_memory::ExecutionV1};
use cgx2_sema::ordinary_memory::compile_source;

const SWEEP: &str =
    include_str!("../../cgx2-syntax/ordinary-recovery-roots-evidence/fixtures/sweep.cg");

fn balanced(execution: &ExecutionV1) {
    let e = execution.evidence.as_ref().unwrap();
    assert_eq!(e.roots_registered, e.roots_released, "{execution:?}");
    assert_eq!(e.baseline_roots, e.final_roots);
    assert_eq!(e.cleanup_failures, 0);
    assert_eq!(e.guard_entries, e.guard_exits);
    assert!(execution
        .memory
        .as_ref()
        .unwrap()
        .regions
        .iter()
        .skip(1)
        .all(|r| !r.is_open));
}

#[cfg(feature = "failure-injection-test")]
#[test]
fn historical_reserves_10_11_21_are_recoverable_and_balanced() {
    let program = compile_source(SWEEP).unwrap();
    for index in [11, 10, 21] {
        let execution =
            cgx2_runtime::ordinary_memory::run_with_host_failure_v1(program.runtime(), &[], index);
        assert!(
            matches!(
                execution.result,
                Ok(Snapshot::Integer(-10 | 0 | -1 | -2 | -3 | -4 | -5 | 42))
            ),
            "reserve {index}: {execution:?}"
        );
        balanced(&execution);
    }
}

#[test]
fn unchanged_nested_region_guard_sweep_baseline() {
    let execution = compile_source(SWEEP).unwrap().execute(&[]).unwrap();
    assert_eq!(execution.result, Ok(Snapshot::Integer(42)));
    balanced(&execution);
}

fn inline(body: &str) -> String {
    format!("fn main() -> i64 effects(ManagedAlloc, Mutate):\n    match memory.try_in(memory.default_region(), [20, 22]):\n        Result.Ok(xs):\n{body}\n        Result.Err(error):\n            -1\n")
}

#[test]
fn inline_binding_reads_reassignment_and_nested_return_keep_distinct_obligations() {
    for (body, expected) in [
        ("            42", 42),
        ("            length(xs) + length(xs) + 38", 42),
        ("            let alias = xs\n            length(xs) + length(alias) + 38", 42),
        ("            if true:\n                return length(xs) + 40\n            else:\n                let ignored = 0\n            0", 42),
        ("            let mut alias = xs\n            alias = [1]\n            length(xs) + length(alias) + 39", 42),
    ] {
        let source = inline(body);
        let execution = compile_source(&source).unwrap().execute(&[]).unwrap();
        assert_eq!(execution.result, Ok(Snapshot::Integer(expected)), "{source}");
        balanced(&execution);
    }
}

#[test]
fn user_arm_arithmetic_failure_keeps_its_primary_diagnostic() {
    let source = inline("            42 / 0");
    let execution = compile_source(&source).unwrap().execute(&[]).unwrap();
    let error = execution.result.as_ref().unwrap_err();
    assert_eq!(error.code, "CGR006");
    assert_eq!(&source[error.loc.start..error.loc.end], "42 / 0");
    balanced(&execution);
}

#[test]
fn returned_payload_and_error_carrier_survive_binding_cleanup() {
    for (ty, body, read) in [
        ("List<i64>", "xs", "length(values()) + 40"),
        ("List<i64>", "if true:\n                return xs\n            else:\n                let ignored = 0\n            xs", "length(values()) + 40"),
        ("Result<i64, List<i64>>", "Result.Err(xs)", "match values():\n        Result.Ok(n):\n            n\n        Result.Err(xs):\n            length(xs) + 40"),
    ] {
        let fallback = if ty.starts_with("Result") { "Result.Ok(0)" } else { "[0]" };
        let source = format!("fn values() -> {ty} effects(ManagedAlloc):\n    match memory.try_in(memory.default_region(), [20, 22]):\n        Result.Ok(xs):\n            {body}\n        Result.Err(error):\n            {fallback}\nfn main() -> i64 effects(ManagedAlloc):\n    {read}\n");
        let execution = compile_source(&source).unwrap().execute(&[]).unwrap();
        assert_eq!(execution.result, Ok(Snapshot::Integer(42)), "{source}");
        balanced(&execution);
    }
}

#[test]
fn copy_argument_and_user_arm_primary_errors_stay_outside_allocation_recovery() {
    for source in [
        "fn main() -> i64 effects(ManagedAlloc):\n    match memory.copy_to(memory.default_region(), [42 / 0]):\n        Result.Ok(xs):\n            42\n        Result.Err(error):\n            -5\n",
        "fn main() -> i64 effects(ManagedAlloc):\n    match memory.copy_to(memory.default_region(), [42]):\n        Result.Ok(xs):\n            42 / 0\n        Result.Err(error):\n            -5\n",
    ] {
        let execution = compile_source(source).unwrap().execute(&[]).unwrap();
        let failure = execution.result.as_ref().unwrap_err();
        assert_eq!(failure.code, "CGR006");
        assert_eq!(&source[failure.loc.start..failure.loc.end], "42 / 0");
        assert_eq!(execution.evidence.as_ref().unwrap().recovered, 0);
        balanced(&execution);
    }
}

#[test]
fn allocation_failure_in_a_separate_user_arm_is_not_recovered_by_copy() {
    let source = "fn main() -> i64 effects(ManagedAlloc):\n    match memory.copy_to(memory.default_region(), [42]):\n        Result.Ok(xs):\n            no_gc(budget: 0):\n                let ys = [1]\n                return length(ys)\n            42\n        Result.Err(error):\n            -5\n";
    let execution = compile_source(source).unwrap().execute(&[]).unwrap();
    assert_eq!(execution.result.as_ref().unwrap_err().code, "CGR090");
    assert_eq!(execution.evidence.as_ref().unwrap().recovered, 0);
    balanced(&execution);
}

#[test]
fn copy_evaluates_its_argument_once_and_restores_the_allocation_context() {
    let source = "fn values(counter: managed.Ref<i64>) -> List<i64> effects(ManagedAlloc, ReadManaged, Mutate):\n    managed.set(counter, managed.get(counter) + 1)\n    [20, 22]\nfn main() -> i64 effects(ManagedAlloc, ReadManaged, Mutate):\n    let counter = managed.ref(0)\n    match memory.copy_to(memory.default_region(), values(counter)):\n        Result.Ok(xs):\n            managed.get(counter) + length(xs) + 39\n        Result.Err(error):\n            -5\n";
    let execution = compile_source(source).unwrap().execute(&[]).unwrap();
    assert_eq!(execution.result, Ok(Snapshot::Integer(42)));
    balanced(&execution);
}
