use cgx2_cli::{run_checked_source, Value};

#[test]
fn local_calls_share_manual_region_session_and_conversion_state() {
    let source = "fn build() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    let r = memory.region(mode: manual, limit: 256)\n    let o = memory.owned_bytes(8)\n    let m = memory.promote_owned(r, o)\n    let copy = memory.copy_to_owned(m)\n    memory.drop(m)\n    memory.drop(copy)\n    memory.collect(r)\n    memory.close(r)\nfn main() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    build()\n    build()\n";
    let (result, report) = run_checked_source(source, &[]).unwrap();
    assert_eq!(result.value, Value::Unit);
    let report = report.unwrap();
    assert_eq!(report.regions.len(), 3);
    assert_eq!(report.totals.collection_count, 2);
    assert!(report.regions.iter().skip(1).all(|region| !region.is_open));
}

#[test]
fn no_gc_budget_failure_inside_call_has_operation_span() {
    let source = "fn build() -> Unit effects(ManagedAlloc):\n    let o = memory.owned_bytes(8)\nfn main() -> Unit effects(ManagedAlloc):\n    no_gc(budget: 0):\n        build()\n";
    let errors = run_checked_source(source, &[]).unwrap_err();
    assert_eq!(errors[0].code, "CGR013");
    let span = errors[0].span.unwrap();
    assert_eq!(
        source[span.start..span.end].trim_end(),
        "let o = memory.owned_bytes(8)"
    );
}
