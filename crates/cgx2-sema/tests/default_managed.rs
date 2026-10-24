use cgx2_sema::default_managed::compile_source;
#[test]
fn ordinary_heap_helper_and_owner_share_guard_without_managed_marker() {
    let source = "record R:\n    value: i64\n\nfn heap() -> i64:\n    let xs = [1, 2]\n    length(xs)\n\nfn main() -> i64:\n    let old = owned(R(value: 7))\n    no_gc(budget: 0):\n        heap()\n    old.value\n";
    let p = cgx2_sema::default_managed::hybrid::compile_source(source).unwrap();
    let e = p.execute().unwrap();
    assert_eq!(e.result.unwrap_err().code, "CGR090");
    assert_eq!(e.owned.allocations, e.owned.releases);
    assert_eq!(e.managed.final_roots, 0);
    assert_eq!(e.memory.totals.collection_count, 0);
}
#[test]
fn ordinary_heap_guard_needs_no_explicit_managed_marker() {
    let source = "fn heap() -> i64:\n let xs = [1, 2]\n length(xs)\n\nfn main() -> i64:\n no_gc(budget: 0):\n  return heap()\n 0\n";
    let module = cgx2_syntax::parse_module(source).unwrap();
    assert!(cgx2_sema::default_managed::explicit(&module));
    let e = run(source);
    assert_eq!(e.result.unwrap_err().code, "CGR090");
    assert_eq!(e.memory.totals.collection_count, 0);
    assert_eq!(e.evidence.final_roots, 0);
}
#[test]
fn argument_errors_keep_legacy_code_message_and_argument_span() {
    for (index, message) in [
        ("-1", "arg index must be nonnegative"),
        ("2", "arg index 2 is out of range"),
    ] {
        let source =
            format!("fn main() -> String effects(ReadArgs) caps(ArgsRead):\n arg({index})\n");
        let e = run(&source);
        let error = e.result.unwrap_err();
        assert_eq!(error.code, "CGR007");
        assert_eq!(error.message, message);
        assert_eq!(&source[error.loc.start..error.loc.end], index);
        assert_eq!(e.evidence.final_roots, 0);
    }
}
#[test]
fn script_effects_describe_real_mutable_operations() {
    let p = compile_source("let cell = managed.ref(1)\nmanaged.set(cell, 2)\nmanaged.get(cell)\n")
        .unwrap();
    let p = p.program();
    let f = &p.functions[p.entry];
    let names = f
        .behavioral_effects
        .iter()
        .map(|i| p.strings[*i].as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["ManagedAlloc", "Mutate", "ReadManaged"]);
}
#[test]
fn hybrid_read_args_rejects_before_using_empty_argument_state() {
    let source = "record R:\n    value: i64\n\nfn heap() -> i64 effects(ReadArgs, ManagedAlloc) caps(ArgsRead):\n let unused = managed.try_alloc(1)\n length(arg(0))\n\nfn main() -> i64:\n let old = owned(R(value: 7))\n heap() + old.value\n";
    let errors = cgx2_sema::default_managed::hybrid::compile_source(source).unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("argument bridge")),
        "{errors:?}"
    );
}
#[test]
fn early_return_releases_nested_locals_and_return_roots() {
    let e = run("fn helper() -> List<i64>:\n let earlier = [1, 2]\n if true:\n  let local = [3, 4]\n  return local\n else:\n  return earlier\n\nfn main() -> i64:\n length(helper())\n");
    assert_eq!(e.result.unwrap().to_string(), "2");
    assert_eq!(e.evidence.final_roots, 0);
    assert_eq!(e.evidence.roots_registered, e.evidence.roots_released);
}
#[test]
fn ref_graph_traversal_handles_more_than_snapshot_depth() {
    let e = run("record Node:\n    next: managed.Ref<Option<Node>>\n\nfn chain() -> Unit effects(ManagedAlloc, Mutate):\n let root: managed.Ref<Option<Node>> = managed.ref(Option<Node>.None())\n let mut current = root\n for i in range(0, 100):\n  let next: managed.Ref<Option<Node>> = managed.ref(Option<Node>.None())\n  managed.set(current, Option.Some(Node(next: next)))\n  current = next\n managed.set(current, Option.Some(Node(next: root)))\n\nfn main() -> i64 effects(ManagedAlloc, Mutate, Collect) caps(MemoryCollect):\n chain()\n managed.collect()\n 42\n");
    assert_eq!(e.result.unwrap().to_string(), "42");
    assert_eq!(e.memory.totals.committed_bytes, 0);
    assert_eq!(e.evidence.final_roots, 0);
}
#[test]
fn wire_roundtrip_preserves_every_fixture_model_and_rejects_truncation() {
    for source in [
        include_str!("../../cgx2-syntax/default-managed-evidence/shapes.cg"),
        include_str!("../../cgx2-syntax/default-managed-evidence/alias.cg"),
        include_str!("../../cgx2-syntax/default-managed-evidence/recover.cg"),
        include_str!("../../cgx2-syntax/default-managed-evidence/cycle.cg"),
        include_str!("../../cgx2-syntax/default-managed-evidence/region-limit.cg"),
    ] {
        let p = compile_source(source).unwrap();
        let wire = p.program().wire_v1();
        assert_eq!(
            &cgx2_runtime::default_managed::model::Program::decode_wire_v1(&wire).unwrap(),
            p.program()
        );
        for cut in [0, 1, 4, 5, wire.len() / 2, wire.len() - 1] {
            assert!(
                cgx2_runtime::default_managed::model::Program::decode_wire_v1(&wire[..cut])
                    .is_err()
            );
        }
        let mut extra = wire.clone();
        extra.push(0);
        assert!(cgx2_runtime::default_managed::model::Program::decode_wire_v1(&extra).is_err());
    }
}
#[test]
fn allocation_failure_keeps_original_utf8_expression_span() {
    let source="# λ🙂\nfn main() -> i64 effects(ManagedAlloc):\n    no_gc(budget: 0):\n        managed.ref(1)\n    42\n";
    let e = run(source);
    let error = e.result.unwrap_err();
    assert_eq!(error.code, "CGR090");
    assert_eq!(&source[error.loc.start..error.loc.end], "managed.ref(1)");
    assert_eq!(e.evidence.final_roots, 0);
}
#[test]
fn failed_managed_argument_releases_earlier_owner() {
    let p = cgx2_sema::default_managed::hybrid::compile_source(include_str!(
        "../../cgx2-syntax/default-managed-evidence/failed-arguments.cg"
    ))
    .unwrap();
    let e = p.execute().unwrap();
    assert_eq!(e.result.unwrap_err().code, "CGR006");
    assert_eq!(e.owned.allocations, 1);
    assert_eq!(e.owned.releases, 1);
    assert_eq!(e.owned.live_bytes, 0);
    assert_eq!(e.managed.final_roots, 0);
}
#[test]
fn real_region_limit_recovers() {
    let e = run(include_str!(
        "../../cgx2-syntax/default-managed-evidence/region-limit.cg"
    ));
    assert_eq!(e.result.unwrap().to_string(), "42");
    assert!(e.evidence.recovered > 0);
    assert_eq!(e.evidence.final_roots, 0);
}
#[test]
fn intrinsic_and_heap_result_can_replace_same_typed_cell() {
    let e=run("fn main() -> i64 effects(ManagedAlloc, ReadManaged, Mutate):\n    let r = managed.try_alloc(1)\n    let cell = managed.ref(r)\n    managed.set(cell, Result.Ok(42))\n    match managed.get(cell):\n        Result.Ok(value):\n            value\n        Result.Err(error):\n            0\n");
    assert_eq!(e.result.unwrap().to_string(), "42");
    assert_eq!(e.evidence.final_roots, 0);
}
#[test]
fn expression_tails_in_loops_do_not_retain_dead_roots() {
    let e=run("record Box:\n    value: i64\nfn main() -> i64:\n    for i in range(0, 20000):\n        Box(value: i)\n    42\n");
    assert_eq!(e.result.unwrap().to_string(), "42");
    assert!(e.memory.totals.total_collected_bytes > 0);
    assert_eq!(e.evidence.final_roots, 0);
    assert_eq!(e.evidence.roots_registered, e.evidence.roots_released);
}
#[test]
fn owned_and_managed_share_actual_guarded_session() {
    let p = cgx2_sema::default_managed::hybrid::compile_source(include_str!(
        "../../cgx2-syntax/default-managed-evidence/hybrid.cg"
    ))
    .unwrap();
    let e = p.execute().unwrap();
    assert_eq!(e.result.unwrap().to_string(), "49");
    assert_eq!(e.owned.allocations, 1);
    assert_eq!(e.owned.releases, 1);
    assert_eq!(e.owned.result_err, 1);
    assert_eq!(e.managed.recovered, 1);
    assert_eq!(e.memory.totals.collection_count, 0);
    assert_eq!(e.managed.final_roots, 0);
}
fn run(source: &str) -> cgx2_runtime::default_managed::Execution {
    let p = compile_source(source).unwrap_or_else(|e| panic!("{e:?}"));
    p.execute(&[]).unwrap()
}
#[test]
fn real_graph_record_and_persistent_list() {
    let execution=run("record Box<T>:\n    value: T\n\nfn wrap<T>(x: T) -> Box<T>:\n    Box<T>(value: x)\n\nfn main() -> i64:\n    let original = [10, 20]\n    let changed = append(original, 30)\n    let boxed = wrap(changed)\n    length(original) + length(boxed.value)\n");
    assert_eq!(execution.result.unwrap().to_string(), "5");
    assert!(execution.evidence.allocations >= 3);
    assert_eq!(execution.evidence.final_roots, 0);
    assert_eq!(
        execution.evidence.roots_registered,
        execution.evidence.roots_released
    );
}
#[test]
fn recovery_catches_nested_allocation() {
    let execution=run("fn main() -> i64 effects(ManagedAlloc):\n    no_gc(budget: 0):\n        let outcome = managed.try_alloc([1, 2])\n        match outcome:\n            Result.Ok(xs):\n                return length(xs)\n            Result.Err(error):\n                return 42\n    0\n");
    assert_eq!(execution.result.unwrap().to_string(), "42");
    assert_eq!(execution.evidence.recovered, 1);
    assert_eq!(execution.memory.totals.collection_count, 0);
    assert_eq!(execution.evidence.final_roots, 0);
}
#[test]
fn typed_ref_alias() {
    let execution=run("fn main() -> i64 effects(ManagedAlloc, ReadManaged, Mutate):\n    let cell = managed.ref(1)\n    let alias = cell\n    managed.set(alias, 42)\n    managed.get(cell)\n");
    assert_eq!(execution.result.unwrap().to_string(), "42");
    assert_eq!(execution.evidence.final_roots, 0);
}
#[test]
fn transitive_collect_rejects() {
    let source="fn collect_now() -> Unit effects(Collect) caps(MemoryCollect):\n    managed.collect()\n\nfn main() -> i64 effects(Collect) caps(MemoryCollect):\n    no_gc(budget: 100):\n        collect_now()\n    1\n";
    let errors = compile_source(source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("forbidden transitively")),
        "{errors:?}"
    );
}
#[test]
fn actual_ref_cycle_and_reclamation() {
    let execution=run("record Node:\n    next: managed.Ref<Option<Node>>\n\nfn cycle() -> Unit effects(ManagedAlloc, Mutate):\n    let cell: managed.Ref<Option<Node>> = managed.ref(Option<Node>.None())\n    let node = Node(next: cell)\n    managed.set(cell, Option.Some(node))\n\nfn main() -> i64 effects(ManagedAlloc, Mutate, Collect) caps(MemoryCollect):\n    cycle()\n    managed.collect()\n    42\n");
    assert_eq!(execution.result.unwrap().to_string(), "42");
    assert_eq!(execution.memory.totals.committed_bytes, 0);
    assert_eq!(execution.evidence.final_roots, 0);
}
#[test]
fn recovery_preserves_primary_arithmetic() {
    let execution=run("fn main() -> i64 effects(ManagedAlloc):\n    let value = managed.try_alloc(1 / 0)\n    1\n");
    let err = execution.result.unwrap_err();
    assert_eq!(err.code, "CGR006");
    assert_eq!(execution.evidence.recovered, 0);
    assert_eq!(execution.evidence.final_roots, 0);
}
#[test]
fn ref_public_snapshot_rejected() {
    let e = compile_source(
        "fn main() -> managed.Ref<i64> effects(ManagedAlloc):\n    managed.ref(1)\n",
    )
    .unwrap_err();
    assert!(e.iter().any(|e| e.message.contains("public result")));
}
#[test]
fn heap_pressure_reclaims_dead_locals_and_protects_arguments() {
    let execution = run(include_str!(
        "../../cgx2-syntax/default-managed-evidence/pressure.cg"
    ));
    assert_eq!(execution.result.unwrap().to_string(), "52");
    assert!(execution.memory.totals.collection_count > 0);
    assert!(execution.memory.totals.total_collected_bytes > 0);
    assert_eq!(execution.evidence.final_roots, 0);
    assert!(execution.evidence.memory_effects_v1.may_allocate_managed);
    assert!(execution.evidence.behavioral_effects.is_empty());
}
#[test]
fn unicode_exact_mebibyte() {
    let execution = run(include_str!(
        "../../cgx2-syntax/default-managed-evidence/unicode.cg"
    ));
    assert_eq!(execution.result.unwrap().to_string(), "262144");
    assert_eq!(execution.evidence.final_roots, 0);
}
#[test]
fn every_allocation_error_arm_is_typed() {
    let execution = run(include_str!(
        "../../cgx2-syntax/default-managed-evidence/recover.cg"
    ));
    assert_eq!(execution.result.unwrap().to_string(), "42");
}
fn warmed(budget: usize, inside: &str) -> String {
    format!("fn main() -> i64 effects(ManagedAlloc):\n    [managed.ref(0), managed.ref(0), managed.ref(0), managed.ref(0)]\n    no_gc(budget: {budget}):\n{inside}    42\n")
}
#[test]
fn exact_allocation_and_nonrefund_budgets() {
    for (budget, recovered) in [(0, 1), (225, 1), (226, 0)] {
        let e = run(&warmed(
            budget,
            "        let value = managed.try_alloc(managed.ref(1))\n",
        ));
        assert_eq!(e.result.unwrap().to_string(), "42");
        assert_eq!(e.evidence.recovered, recovered, "budget {budget}");
        assert_eq!(e.memory.totals.collection_count, 0);
        assert_eq!(e.evidence.final_roots, 0);
    }
    let e = run(&warmed(
        451,
        "        managed.try_alloc(managed.ref(1))\n        managed.try_alloc(managed.ref(2))\n",
    ));
    assert_eq!(e.result.unwrap().to_string(), "42");
    assert_eq!(e.evidence.recovered, 1);
    assert_eq!(e.memory.totals.collection_count, 0);
}
#[test]
fn root_bookkeeping_is_inside_recovery() {
    for (budget, recovered) in [(0, 1), (63, 1), (64, 0)] {
        let e=run(&format!("fn main() -> i64 effects(ManagedAlloc):\n    let cell = managed.ref(1)\n    no_gc(budget: {budget}):\n        let outcome = managed.try_alloc(cell)\n    42\n"));
        assert_eq!(e.result.unwrap().to_string(), "42");
        assert_eq!(e.evidence.recovered, recovered, "root budget {budget}");
        assert_eq!(e.evidence.allocations, 1);
        assert_eq!(e.evidence.final_roots, 0);
    }
}
#[test]
fn nested_guards_charge_outer_and_inner() {
    for (outer, recovered) in [(257, 1), (258, 0)] {
        let e=run(&warmed(outer,"        no_gc(budget: 226):\n            let outcome = managed.try_alloc(managed.ref(1))\n"));
        assert_eq!(e.result.unwrap().to_string(), "42");
        assert_eq!(e.evidence.recovered, recovered);
        assert_eq!(e.evidence.guard_entries, 2);
        assert_eq!(e.evidence.guard_exits, 2);
        assert_eq!(e.memory.totals.collection_count, 0);
    }
}
#[cfg(feature = "failure-injection-test")]
#[test]
fn synthetic_graph_and_root_failures_cleanup() {
    let source="fn main() -> i64 effects(ManagedAlloc):\n    let result = managed.try_alloc(managed.ref(1))\n    match result:\n        Result.Ok(value):\n            1\n        Result.Err(error):\n            42\n";
    let p = compile_source(source).unwrap();
    let mut recovered = 0;
    for index in 2..16 {
        let session = cgx2_runtime::Session::try_new_with_host_failure_at_reserve(index).unwrap();
        let e = p.execute_with_session(&[], session).unwrap();
        assert_eq!(e.evidence.final_roots, 0, "fault {index}");
        if e.evidence.recovered > 0 {
            assert_eq!(e.result.unwrap().to_string(), "42");
            recovered += 1;
        } else if let Err(f) = e.result {
            assert_eq!(f.code, "CGR090");
        }
    }
    assert!(recovered >= 3);
}
