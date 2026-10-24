use cgx2_sema::default_managed::{compile_source, compile_source_bundle_v1};

fn prints42(source: &str) {
    let p = compile_source(source).unwrap();
    assert_eq!(p.execute(&[]).unwrap().result.unwrap().to_string(), "42");
}
#[test]
fn local_allocationerror_variant_retains_schema1_identity() {
    prints42(include_str!("../../../plans/release-completion/s2b-default-managed-review/repros/core-allocationerror.cg"));
    prints42("variant AllocationError<T>:\n    Mine(T)\n\nfn value(x: AllocationError<i64>) -> AllocationError<i64> effects():\n    x\n\nfn main() -> i64 effects():\n    let x: AllocationError<i64> = AllocationError<i64>.Mine(42)\n    match value(x):\n        AllocationError<i64>.Mine(n):\n            n\n");
    prints42("module app\nvariant AllocationError:\n    Mine(i64)\n\nfn value(x: app.AllocationError) -> app.AllocationError effects():\n    x\n\nfn main() -> i64 effects():\n    let x: app.AllocationError = AllocationError.Mine(42)\n    match value(x):\n        app.AllocationError.Mine(n):\n            n\n");
    prints42("module app\nvariant AllocationError<T>:\n    Mine(T)\n\nfn value(x: app.AllocationError<i64>) -> app.AllocationError<i64> effects():\n    x\n\nfn main() -> i64 effects():\n    let x: app.AllocationError<i64> = app.AllocationError<i64>.Mine(42)\n    match value(x):\n        app.AllocationError<i64>.Mine(n):\n            n\n");
}
#[test]
fn imported_allocationerror_generic_variant_is_preserved() {
    let lib = "module app.lib\npub variant AllocationError<T>:\n    Mine(T)\n\npub fn value(x: AllocationError<i64>) -> AllocationError<i64> effects():\n    x\n";
    let main = "module app.main\nimport app.lib\nfn main() -> i64 effects():\n    let x: app.lib.AllocationError<i64> = app.lib.AllocationError<i64>.Mine(42)\n    match app.lib.value(x):\n        app.lib.AllocationError<i64>.Mine(n):\n            n\n";
    let p = compile_source_bundle_v1(
        &[("lib".into(), lib.into()), ("main".into(), main.into())],
        "main",
    )
    .unwrap();
    assert_eq!(p.execute(&[]).unwrap().result.unwrap().to_string(), "42");
}
#[test]
fn nominal_record_and_real_recovery_error_are_distinct() {
    prints42("record AllocationError:\n    value: i64\n\nfn echo(x: AllocationError) -> AllocationError effects():\n    x\n\nfn main() -> i64 effects():\n    let x = echo(AllocationError(value: 42))\n    x.value\n");
    prints42("variant AllocationError:\n    Mine(i64)\n\nfn main() -> i64 effects(ManagedAlloc):\n    let result: Result<i64,cgx2.core.AllocationError> = managed.try_alloc(42)\n    match result:\n        Result<i64,cgx2.core.AllocationError>.Ok(x):\n            x\n        Result<i64,cgx2.core.AllocationError>.Err(e):\n            0\n");
    let source = "variant AllocationError:\n    Mine(i64)\nfn main() -> i64 effects(ManagedAlloc):\n    let result: Result<i64,AllocationError> = managed.try_alloc(42)\n    42\n";
    assert!(compile_source(source).is_err());
    assert!(compile_source("fn main() -> i64 effects(ManagedAlloc):\n    let x: Result<i64,i64> = managed.try_alloc(42)\n    42\n").is_err());
}
