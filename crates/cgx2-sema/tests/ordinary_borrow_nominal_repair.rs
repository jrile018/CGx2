use cgx2_runtime::default_managed::Snapshot;
use cgx2_sema::ordinary_memory::{compile_source, compile_source_bundle_v1};

fn region(body: &str) -> String {
    format!("fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n{body}\n        Result.Err(error):\n            -1\n")
}

fn integer(source: &str, expected: i64) {
    let program = compile_source(source).unwrap();
    let execution = program.execute(&[]).unwrap();
    assert_eq!(execution.result.unwrap(), Snapshot::Integer(expected));
    let evidence = execution.evidence.unwrap();
    assert_eq!(evidence.baseline_roots, evidence.final_roots);
    assert_eq!(evidence.cleanup_failures, 0);
}

fn exact(source: &str, code: &str, needle: &str) {
    let errors = compile_source(source).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, code, "{errors:?}");
    let start = source.find(needle).unwrap();
    assert_eq!(
        errors[0].span,
        cgx2_syntax::Span {
            start,
            end: start + needle.len()
        }
    );
}

#[test]
fn retained_local_record_borrow() {
    integer(include_str!("../../../plans/release-completion/s2b-ordinary-memory-review/repros/core-nominal/record-borrow.cg"), 42);
}

#[test]
fn local_nominal_precedes_intrinsic_with_named_and_qualified_arguments() {
    for constructor in ["borrow(value: 42)", "app.borrow(value: 42)"] {
        integer(
            &format!(
                "module app\nrecord borrow:\n    value: i64\n{}",
                region(&format!(
                    "            let x = {constructor}\n            x.value"
                ))
            ),
            42,
        );
    }
}

#[test]
fn generic_borrow_and_nested_heap_fields() {
    integer(
        &format!(
            "record borrow<T>:\n    value: T\n{}",
            region("            let x = borrow(value: \"秘密\")\n            length(x.value) + 40")
        ),
        42,
    );
    integer(&format!("record borrow<T>:\n    value: T\nrecord Box:\n    text: String\n{}", region("            let x = borrow<Box>(value: Box(text: \"秘密\"))\n            let box = x.value\n            length(box.text) + 40")), 42);
}

#[test]
fn qualified_imported_record_coexists_with_intrinsic() {
    let library = "module util\npub record borrow<T>:\n    value: T\n";
    let app = format!("module app\nimport util\n{}", region("            match memory.try_in(borrow(r), util.borrow<String>(value: \"秘密\")):\n                Result.Ok(x):\n                    length(x.value) + 40\n                Result.Err(error):\n                    -2"));
    let program = compile_source_bundle_v1(
        &[("util".into(), library.into()), ("app".into(), app)],
        "app",
    )
    .unwrap();
    assert_eq!(
        program.execute(&[]).unwrap().result.unwrap(),
        Snapshot::Integer(42)
    );
}

#[test]
fn imported_unparameterized_nominal_call_precedes_intrinsic() {
    let library = "module util\npub record borrow:\n    value: i64\n";
    let app = format!("module app\nimport util\n{}", region("            match memory.try_in(borrow(r), util.borrow(value: 42)):\n                Result.Ok(x):\n                    x.value\n                Result.Err(error):\n                    -2"));
    let program = compile_source_bundle_v1(
        &[("util".into(), library.into()), ("app".into(), app)],
        "app",
    )
    .unwrap();
    assert_eq!(
        program.execute(&[]).unwrap().result.unwrap(),
        Snapshot::Integer(42)
    );
}

#[test]
fn local_and_imported_user_functions_retain_resolution() {
    integer(
        &format!(
            "fn borrow(value: i64) -> i64 effects():\n    value + 1\n{}",
            region("            borrow(41)")
        ),
        42,
    );
    let library = "module util\npub fn borrow(value: i64) -> i64 effects():\n    value + 1\n";
    let app = format!("module app\nimport util\n{}", region("            match memory.try_in(borrow(r), [util.borrow(41)]):\n                Result.Ok(xs):\n                    length(xs) + util.borrow(40)\n                Result.Err(error):\n                    -2"));
    let program = compile_source_bundle_v1(
        &[("util".into(), library.into()), ("app".into(), app)],
        "app",
    )
    .unwrap();
    assert_eq!(
        program.execute(&[]).unwrap().result.unwrap(),
        Snapshot::Integer(42)
    );
}

#[test]
fn constructor_fields_still_type_check() {
    let source = format!(
        "record borrow:\n    value: i64\n{}",
        region("            let x = borrow(value: true)\n            x.value")
    );
    exact(&source, "CGT003", "true");
}

#[test]
fn named_record_cannot_forge_a_capability() {
    let source = format!(
        "record borrow:\n    value: i64\n{}",
        region("            memory.collect(borrow(value: 42))\n            42")
    )
    .replace(
        "effects(ManagedAlloc)",
        "effects(ManagedAlloc, Collect) caps(MemoryCollect)",
    );
    let errors = compile_source(&source).unwrap_err();
    assert_eq!(errors.len(), 2, "{errors:?}");
    let start = source.find("borrow(value: 42)").unwrap();
    for error in errors {
        assert_eq!(error.code, "CGT003");
        assert_eq!(
            error.span,
            cgx2_syntax::Span {
                start,
                end: start + "borrow(value: 42)".len()
            }
        );
    }
}

#[test]
fn record_cannot_hide_real_capability() {
    let source = format!(
        "record borrow:\n    value: i64\n{}",
        region("            let x = borrow(value: r)\n            x.value")
    );
    exact(&source, "CGT095", "borrow(value: r)");
}

#[test]
fn nominal_constructor_cannot_erase_named_region_provenance() {
    let source = "record borrow:\n    value: String\nfn leak<R: memory.RegionName>(r: shared<memory.Region<R>>) -> String effects(ManagedAlloc):\n    match memory.try_in(r, borrow(value: \"秘密\")):\n        Result.Ok(x):\n            return x.value\n        Result.Err(error):\n            return \"failed\"\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            42\n        Result.Err(error):\n            -1\n";
    exact(source, "CGT095", "return x.value\n");
}

#[test]
fn genuine_borrow_still_requires_region_and_preserves_escape_checks() {
    exact(
        &region("            let x = borrow(42)\n            42"),
        "CGT095",
        "borrow(42)",
    );
    integer(&region("            match memory.try_in(borrow(r), [42]):\n                Result.Ok(xs):\n                    length(xs) + 41\n                Result.Err(error):\n                    -2"), 42);
}
