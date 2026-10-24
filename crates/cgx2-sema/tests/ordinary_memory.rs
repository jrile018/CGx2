use cgx2_runtime::default_managed::Snapshot;
use cgx2_sema::ordinary_memory::compile_source;
#[test]
fn named_manual_region_generic_helper_and_lexical_close() {
    let source="fn build<R: memory.RegionName>(r: shared<memory.Region<R>>) -> Result<memory.Value<R, List<i64>>, AllocationError> effects(ManagedAlloc):\n    memory.try_in(r, [20, 22])\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match build(borrow(r)):\n                Result.Ok(xs):\n                    length(xs)\n                Result.Err(error):\n                    -2\n        Result.Err(error):\n            -1\n";
    let program = compile_source(source).unwrap();
    let execution = program.execute(&[]).unwrap();
    assert_eq!(execution.result.unwrap(), Snapshot::Integer(2));
    assert_eq!(
        execution.evidence.as_ref().unwrap().baseline_roots,
        execution.evidence.as_ref().unwrap().final_roots
    );
    assert_eq!(execution.evidence.as_ref().unwrap().cleanup_failures, 0);
    assert_eq!(execution.memory.as_ref().unwrap().regions.len(), 2);
    assert!(!execution.memory.as_ref().unwrap().regions[1].is_open);
    assert_eq!(
        execution.memory.as_ref().unwrap().regions[1].collection_count,
        0
    );
}
#[test]
fn failed_guard_entry_is_a_source_result_and_body_is_not_entered() {
    let source="fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 0):\n        Result.Ok(guard):\n            1\n        Result.Err(error):\n            42\n";
    let program = compile_source(source).unwrap();
    let execution =
        cgx2_runtime::ordinary_memory::run_with_host_failure_v1(program.runtime(), &[], 2);
    assert_eq!(execution.result.unwrap(), Snapshot::Integer(42));
    assert_eq!(execution.evidence.as_ref().unwrap().guard_entry_failures, 1);
    assert_eq!(execution.evidence.as_ref().unwrap().guard_entries, 0);
}
#[test]
fn recoverable_session_success_has_exactly_one_session() {
    let source="fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_session():\n        Result.Ok(session):\n            42\n        Result.Err(error):\n            match error:\n                memory.SessionEntryError.HostAllocation(site):\n                    -10\n                memory.SessionEntryError.IdentifierExhausted:\n                    -11\n";
    let execution = compile_source(source).unwrap().execute(&[]).unwrap();
    assert_eq!(execution.result.unwrap(), Snapshot::Integer(42));
    assert_eq!(execution.memory.as_ref().unwrap().regions.len(), 1);
}
#[test]
fn named_region_escape_and_default_callee_relocation_reject() {
    for source in [
        "fn main() -> List<i64> effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match memory.try_in(borrow(r), [42]):\n                Result.Ok(xs):\n                    xs\n                Result.Err(error):\n                    [0]\n        Result.Err(error):\n            [0]\n",
        "fn default_values() -> List<i64> effects(ManagedAlloc):\n    [42]\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match memory.try_in(borrow(r), default_values()):\n                Result.Ok(xs):\n                    length(xs)\n                Result.Err(error):\n                    0\n        Result.Err(error):\n            0\n",
    ] {let errors=compile_source(source).unwrap_err();assert_eq!(errors[0].code,"CGT095");}
}
#[test]
fn entry_contracts_and_resource_fallback_reject() {
    let source="fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_session():\n        Result.Ok(session):\n            42\n        Result.Err(error):\n            length([1])\n";
    assert!(compile_source(source)
        .unwrap_err()
        .iter()
        .any(|e| e.message.contains("fallback")));
}
#[test]
fn session_fallback_preserves_entry_error_and_has_no_synthetic_memory_metrics() {
    let source="fn fallback() -> i64 effects():\n    -10\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_session():\n        Result.Ok(session):\n            42\n        Result.Err(error):\n            match error:\n                memory.SessionEntryError.HostAllocation(site):\n                    fallback()\n                memory.SessionEntryError.IdentifierExhausted:\n                    -11\n";
    let p = compile_source(source).unwrap();
    for (error, expected) in [
        (
            cgx2_runtime::SessionEntryError::HostAllocation {
                site: cgx2_runtime::ReserveSiteV2::SessionRegions,
            },
            -10,
        ),
        (cgx2_runtime::SessionEntryError::IdentifierExhausted, -11),
    ] {
        let execution = cgx2_runtime::ordinary_memory::run_with_entry_error_v1(p.runtime(), error);
        assert_eq!(execution.result.unwrap(), Snapshot::Integer(expected));
        assert!(execution.memory.is_none());
        assert!(execution.evidence.is_none());
    }
}
#[test]
fn copy_to_a_named_region_copies_mutable_graph_storage() {
    let source="fn main() -> i64 effects(ManagedAlloc, Mutate, ReadManaged):\n    let source = managed.ref(1)\n    match memory.try_region(mode: memory.Mode.Auto, limit: 4096, trigger: 128):\n        Result.Ok(r):\n            match memory.copy_to(borrow(r), source):\n                Result.Ok(copy):\n                    managed.set(copy, 41)\n                    managed.get(copy) + managed.get(source)\n                Result.Err(error):\n                    -2\n        Result.Err(error):\n            -1\n";
    let p = compile_source(source).unwrap();
    let execution = p.execute(&[]).unwrap();
    assert_eq!(execution.result.unwrap(), Snapshot::Integer(42));
    assert_eq!(
        execution.evidence.as_ref().unwrap().baseline_roots,
        execution.evidence.as_ref().unwrap().final_roots
    );
    assert_eq!(execution.evidence.as_ref().unwrap().cleanup_failures, 0);
}
#[test]
fn synthetic_option_allocations_stay_in_the_input_region() {
    let source="fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match memory.try_in(borrow(r), [42]):\n                Result.Ok(xs):\n                    match get(xs, 0):\n                        Option.Some(x):\n                            x\n                        Option.None:\n                            -3\n                Result.Err(error):\n                    -2\n        Result.Err(error):\n            -1\n";
    let execution = compile_source(source).unwrap().execute(&[]).unwrap();
    assert_eq!(execution.result.unwrap(), Snapshot::Integer(42));
    assert_eq!(
        execution.memory.as_ref().unwrap().regions[0].committed_bytes,
        0
    );
    assert_eq!(execution.evidence.as_ref().unwrap().cleanup_failures, 0);
}
#[test]
fn qualified_callees_keep_their_default_result_provenance() {
    let source="module app\nfn values() -> List<i64> effects(ManagedAlloc):\n    [42]\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match memory.try_in(borrow(r), app.values()):\n                Result.Ok(xs):\n                    length(xs)\n                Result.Err(error):\n                    -2\n        Result.Err(error):\n            -1\n";
    let errors = compile_source(source).unwrap_err();
    assert_eq!(errors[0].code, "CGT095");
    assert!(errors[0].message.contains("different allocation region"));
}
#[test]
fn session_fallback_rejects_default_capability_even_when_its_erased_stub_is_pure() {
    let source="fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_session():\n        Result.Ok(session):\n            42\n        Result.Err(error):\n            let r = memory.default_region()\n            7\n";
    let errors = compile_source(source).unwrap_err();
    assert_eq!(errors[0].code, "CGT095");
    assert!(errors[0].message.contains("fallback cannot access memory"));
}
#[test]
fn session_fallback_authenticates_transitive_resource_operations_and_utf8_spans() {
    let source="# π λ\nfn fallback() -> i64 effects():\n    let r = memory.default_region()\n    7\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_session():\n        Result.Ok(session):\n            42\n        Result.Err(error):\n            fallback()\n";
    let errors = compile_source(source).unwrap_err();
    assert_eq!(errors[0].code, "CGT095");
    assert!(errors[0].message.contains("fallback cannot access memory"));
    let start = source.find("memory.default_region()").unwrap();
    assert_eq!(
        errors[0].span,
        cgx2_syntax::Span {
            start,
            end: start + "memory.default_region()".len()
        }
    );
}
#[test]
fn capabilities_cannot_be_forged_through_signatures_fields_or_scalar_annotations() {
    let entry="fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_session():\n        Result.Ok(session):\n            42\n        Result.Err(error):\n            -1\n";
    for prefix in [
        "fn forged() -> shared<memory.Region<memory.Default>>:\n    0\n",
        "record Forged:\n    region: shared<memory.Region<memory.Default>>\n",
        "fn forged(x: List<shared<memory.Region<memory.Default>>>) -> i64:\n    0\n",
        "fn forged() -> i64:\n    let region: shared<memory.Region<memory.Default>> = 0\n    0\n",
        "fn forged() -> i64:\n    let region: i64 = memory.default_region()\n    0\n",
    ] {
        let source = format!("{prefix}{entry}");
        let errors = compile_source(&source).unwrap_err();
        assert_eq!(errors[0].code, "CGT095", "{errors:?}");
    }
}
#[test]
fn user_nominal_namespace_precedes_new_builtin_spelling() {
    let source="module memory\nrecord try_region:\n    value: i64\nfn main() -> i64:\n    let result = memory.try_region(value: 42)\n    result.value\n";
    let module = cgx2_syntax::parse_module(source).unwrap();
    assert!(!cgx2_sema::ordinary_memory::profile(&module));
    assert!(cgx2_syntax::ordinary_memory_span(&module).is_none());
    // Leave this declaration to the existing nominal/legacy consumer. A name
    // collision must never be reinterpreted as a trusted region capability.
}
#[test]
fn user_mode_nominal_does_not_authorize_a_compiler_region_mode() {
    let source="module memory\nvariant Mode:\n    Manual\n    Auto\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            42\n        Result.Err(error):\n            -1\n";
    let errors = compile_source(source).unwrap_err();
    assert_eq!(errors[0].code, "CGT095");
    assert!(errors[0].message.contains("user nominal declaration"));
}
#[test]
fn compiler_templates_do_not_add_callable_source_functions() {
    let source="fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_session():\n        Result.Ok(session):\n            cgx2_om_unit()\n            42\n        Result.Err(error):\n            -1\n";
    let errors = compile_source(source).unwrap_err();
    assert_eq!(errors[0].code, "CGT012");
}
