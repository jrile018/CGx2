use cgx2_cli::{run_source, run_source_with_report, Value};

fn report(source: &str) -> cgx2_runtime::MemoryReport {
    let (result, report) = run_source_with_report(source, &[]).unwrap();
    assert_eq!(result.value, Value::Unit);
    report.expect("memory script report")
}

#[test]
fn manual_region_collects_only_explicitly_and_move_is_unique() {
    let report = report("let r = memory.region(mode: manual, limit: 256)\nlet a = memory.owned_bytes(8)\nlet b = memory.move(a)\nmemory.use(b)\nlet x = memory.alloc_bytes(r, 16)\nmemory.collect(r)\nmemory.use(x)\nmemory.drop(x)\nmemory.collect(r)\nmemory.drop(b)\nmemory.close(r)\n");
    assert_eq!(report.regions[1].collection_count, 2);
    assert_eq!(report.regions[1].total_collected_bytes, 80);
    assert!(!report.regions[1].is_open);
    assert_eq!(report.regions[1].committed_bytes, 0);
}

#[test]
fn auto_collects_at_allocation_safepoint_and_regions_are_isolated() {
    let source = "let a = memory.region(mode: auto, limit: 2097152)\nlet b = memory.region(mode: manual, limit: 256)\nlet x = memory.alloc_bytes(a, 1048576)\nlet y = memory.alloc_bytes(b, 1)\nlet z = memory.alloc_bytes(a, 1)\nmemory.use(x)\nmemory.use(y)\nmemory.drop(x)\nmemory.drop(z)\nmemory.collect(a)\nmemory.use(y)\nmemory.drop(y)\nmemory.close(a)\nmemory.close(b)\n";
    let report = report(source);
    assert_eq!(report.regions[1].collection_count, 2);
    assert_eq!(report.regions[2].collection_count, 0);
}

#[test]
fn nested_no_gc_charges_each_guard_across_regions_without_collection() {
    let source = "let a = memory.region(mode: auto, limit: 2097152)\nlet b = memory.region(mode: manual, limit: 256)\nno_gc(budget: 224):\n    let x = memory.alloc_bytes(a, 32)\n    no_gc(budget: 96):\n        let y = memory.alloc_bytes(b, 32)\nmemory.use(x)\nmemory.use(y)\nmemory.drop(x)\nmemory.drop(y)\nmemory.close(a)\nmemory.close(b)\n";
    let report = report(source);
    assert_eq!(report.totals.collection_count, 0);

    for bad in [
        source.replace("budget: 96", "budget: 95"),
        source.replace("budget: 224", "budget: 191"),
    ] {
        let error = run_source(&bad, &[]).unwrap_err();
        assert_eq!(error.code, "CGR013");
        assert!(error.span.is_some());
    }
    let error = run_source("let r = memory.region(mode: manual, limit: 256)\nno_gc(budget: 64):\n    memory.collect(r)\n", &[]).unwrap_err();
    assert_eq!(error.code, "CGO004");
}

#[test]
fn invalid_memory_operations_fail_static_gate_before_runtime() {
    for (source, code) in [
        ("let x = memory.owned_bytes(1)\nlet moved = memory.move(x)\nmemory.use(x)\n", "CGO001"),
        ("let x = memory.owned_bytes(1)\nlet shared = memory.borrow(x)\nlet exclusive = memory.borrow_mut(x)\n", "CGO002"),
        ("let r = memory.region(mode: manual, limit: 128)\nlet x = memory.alloc_bytes(r, 1)\nmemory.close(r)\n", "CGO003"),
        ("let r = memory.region(mode: manual, limit: 128)\nlet x = memory.alloc_bytes(r, 1)\nmemory.drop(x)\nmemory.collect(r)\nmemory.use(x)\n", "CGO001"),
        ("let r = memory.region(mode: manual, limit: 128)\nlet x = memory.alloc_bytes(r, 1)\nlet y = memory.copy(x)\n", "CGM001"),
    ] {
        assert_eq!(run_source(source, &[]).unwrap_err().code, code);
    }
}

#[test]
fn promotion_survives_collection_and_copy_outlives_managed_region() {
    let source = "let a = memory.region(mode: manual, limit: 256)\nlet b = memory.region(mode: manual, limit: 256)\nlet o = memory.owned_bytes(8)\nlet m = memory.promote_owned(a, o)\nlet other = memory.alloc_bytes(b, 1)\nmemory.collect(a)\nmemory.use(m)\nlet copy = memory.copy_to_owned(m)\nmemory.drop(m)\nmemory.collect(a)\nmemory.close(a)\nmemory.use(copy)\nmemory.use(other)\nmemory.drop(copy)\nmemory.drop(other)\nmemory.close(b)\n";
    let report = report(source);
    assert_eq!(report.regions[1].collection_count, 2);
    assert_eq!(report.regions[1].total_collected_bytes, 72);
    assert_eq!(report.regions[2].collection_count, 0);
    assert_eq!(report.regions[2].allocation_failures, 0);
    assert_eq!(report.totals.committed_bytes, 0);
}

#[test]
fn conversion_failures_return_errors_without_success_report() {
    for (source, expected) in [
        ("let r = memory.region(mode: manual, limit: 71)\nlet o = memory.owned_bytes(8)\nlet m = memory.promote_owned(r, o)\n", "CGR013"),
        ("let r = memory.region(mode: manual, limit: 256)\nlet o = memory.owned_bytes(8)\nno_gc(budget: 63):\n    let m = memory.promote_owned(r, o)\n", "CGR013"),
        ("let r = memory.region(mode: manual, limit: 256)\nlet m = memory.alloc_bytes(r, 8)\nno_gc(budget: 39):\n    let copy = memory.copy_to_owned(m)\n", "CGR013"),
    ] {
        assert_eq!(run_source_with_report(source, &[]).unwrap_err().code, expected);
    }
}
