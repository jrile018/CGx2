use cgx2_runtime::default_managed::Snapshot;
use cgx2_sema::ordinary_memory::compile_source;

fn balanced(source: &str, expected: i64) {
    let execution = compile_source(source).unwrap().execute(&[]).unwrap();
    assert_eq!(execution.result.unwrap(), Snapshot::Integer(expected));
    let evidence = execution.evidence.unwrap();
    assert_eq!(evidence.roots_registered, evidence.roots_released);
    assert_eq!(evidence.baseline_roots, evidence.final_roots);
    assert_eq!(evidence.cleanup_failures, 0);
    assert_eq!(evidence.guard_entries, evidence.guard_exits);
    for region in execution.memory.unwrap().regions.iter().skip(1) {
        assert!(!region.is_open);
        assert_eq!(region.committed_bytes, 0);
    }
}

#[test]
fn shipped_outer_assignment_repro_releases_the_actual_outer_registration() {
    balanced(include_str!("../../../plans/release-completion/s2b-ordinary-memory-review/repros/outer-root-assignment.cg"), 1);
}

#[test]
fn repeated_outer_heap_and_scalar_assignments_survive_nested_regions_and_guards() {
    balanced(
        include_str!(
            "../../cgx2-syntax/ordinary-roots-repair-evidence/fixtures/nested-assignment.cg"
        ),
        42,
    );
}

#[test]
fn outer_assignments_clean_up_on_return_and_primary_error() {
    let source = include_str!(
        "../../cgx2-syntax/ordinary-roots-repair-evidence/fixtures/nested-assignment.cg"
    );
    balanced(&source.replace("total = 42", "return 42"), 42);
    let failing = source.replace("total = 42", "total = 42 / 0");
    let execution = compile_source(&failing).unwrap().execute(&[]).unwrap();
    assert_eq!(execution.result.unwrap_err().code, "CGR006");
    let evidence = execution.evidence.unwrap();
    assert_eq!(evidence.roots_registered, evidence.roots_released);
    assert_eq!(evidence.baseline_roots, evidence.final_roots);
    assert_eq!(evidence.cleanup_failures, 0);
    assert_eq!(evidence.guard_entries, evidence.guard_exits);
    assert!(execution
        .memory
        .unwrap()
        .regions
        .iter()
        .skip(1)
        .all(|r| !r.is_open));
}

#[test]
fn copy_publication_is_counted_exactly_once_on_success_and_not_on_failure() {
    balanced(
        include_str!("../../cgx2-syntax/ordinary-memory-evidence/fixtures/copy-cycle.cg"),
        42,
    );
    let failure = include_str!("../../cgx2-syntax/ordinary-memory-evidence/fixtures/copy-cycle.cg")
        .replace("limit: 16384", "limit: 0");
    balanced(&failure, -2);
}

#[test]
fn region_local_graph_still_cannot_escape_through_outer_assignment() {
    let source = "fn main() -> i64 effects(ManagedAlloc, Mutate):\n    let mut xs = [1]\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match memory.try_in(borrow(r), [2]):\n                Result.Ok(local):\n                    xs = local\n                    0\n                Result.Err(error):\n                    0\n        Result.Err(error):\n            0\n    length(xs)\n";
    assert!(compile_source(source).is_err());
}

#[test]
fn outer_graph_result_and_auto_region_assignments_keep_their_registrations() {
    let source = include_str!(
        "../../cgx2-syntax/ordinary-roots-repair-evidence/fixtures/nested-assignment.cg"
    );
    balanced(
        &source.replace(
            "memory.Mode.Manual, limit: 16384",
            "memory.Mode.Auto, limit: 16384, trigger: 1",
        ),
        42,
    );
    let helper = "fn replace() -> List<i64> effects(ManagedAlloc, Mutate):\n    let mut xs = [1]\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            xs = [20, 22]\n            xs\n        Result.Err(error):\n            xs\nfn main() -> i64 effects(ManagedAlloc, Mutate):\n    length(replace())\n";
    balanced(helper, 2);
}

#[test]
fn overwritten_outer_graph_is_collectible_before_the_region_arm_exits() {
    let source = "fn main() -> i64 effects(ManagedAlloc, Mutate, Collect) caps(MemoryCollect):\n    let mut xs = [1]\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            xs = [2, 3]\n            memory.collect(memory.default_region())\n            0\n        Result.Err(error):\n            0\n    length(xs)\n";
    balanced(source, 2);
    let execution = compile_source(source).unwrap().execute(&[]).unwrap();
    assert!(execution.memory.unwrap().regions[0].total_collected_bytes > 0);
}

#[cfg(feature = "failure-injection-test")]
#[test]
fn source_copy_and_assignment_reserve_failures_keep_exact_root_deltas() {
    for source in [
        include_str!(
            "../../cgx2-syntax/ordinary-roots-repair-evidence/fixtures/nested-assignment.cg"
        ),
        include_str!("../../cgx2-syntax/ordinary-memory-evidence/fixtures/copy-cycle.cg"),
    ] {
        let program = compile_source(source).unwrap();
        for fail_at in 1..=64 {
            let mut session = cgx2_runtime::Session::try_new().unwrap();
            session.inject_host_failure_at_reserve(fail_at);
            let execution =
                cgx2_runtime::ordinary_memory::run_with_session(program.runtime(), &[], session);
            let evidence = execution.evidence.unwrap();
            assert_eq!(
                evidence.roots_registered, evidence.roots_released,
                "reserve {fail_at}: {:?}",
                execution.result
            );
            assert_eq!(
                evidence.baseline_roots, evidence.final_roots,
                "reserve {fail_at}"
            );
            assert_eq!(evidence.cleanup_failures, 0, "reserve {fail_at}");
            assert_eq!(
                evidence.guard_entries, evidence.guard_exits,
                "reserve {fail_at}"
            );
            assert!(execution
                .memory
                .unwrap()
                .regions
                .iter()
                .skip(1)
                .all(|r| !r.is_open));
        }
    }
}
