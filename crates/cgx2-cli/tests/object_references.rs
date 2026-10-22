use cgx2_cli::{check_source, run_checked_source, Value};
use cgx2_runtime::{CollectionMode, MemoryReport, Session};
use std::process::Command;

fn source_report(source: &str) -> MemoryReport {
    let (result, report) = run_checked_source(source, &[]).unwrap();
    assert_eq!(result.value, Value::Unit);
    report.unwrap()
}

fn assert_region_parity(source: &str, direct: MemoryReport) {
    let source = source_report(source);
    let actual = &source.regions[1];
    let expected = &direct.regions[1];
    assert_eq!(actual.collection_count, expected.collection_count);
    assert_eq!(actual.total_collected_bytes, expected.total_collected_bytes);
    assert_eq!(actual.allocation_failures, expected.allocation_failures);
    assert_eq!(actual.peak_bytes, expected.peak_bytes);
    assert_eq!(actual.committed_bytes, expected.committed_bytes);
    assert_eq!(actual.live_bytes, expected.live_bytes);
    assert_eq!(actual.limit_bytes, expected.limit_bytes);
    assert_eq!(actual.mode, expected.mode);
    assert_eq!(actual.is_open, expected.is_open);
}

#[test]
fn source_graph_matches_direct_runtime_after_link_clear_and_collection() {
    let mut session = Session::new();
    let r = session.create_region(CollectionMode::Manual, 1024).unwrap();
    let parent = session.alloc_object(r, 8, 1).unwrap();
    session.root(parent).unwrap();
    let child = session.alloc_object(r, 4, 0).unwrap();
    session.root(child).unwrap();
    session.set_reference(parent, 0, Some(child)).unwrap();
    session.unroot(child).unwrap();
    session.collect(r).unwrap();
    let live = session.reference(parent, 0).unwrap().unwrap();
    session.read_bytes(live).unwrap();
    session.set_reference(parent, 0, None).unwrap();
    session.collect(r).unwrap();
    session.unroot(parent).unwrap();
    session.collect(r).unwrap();
    session.close_region(r).unwrap();
    assert_region_parity(
        include_str!("fixtures/object_references.cg"),
        session.memory_report(),
    );
}

#[test]
fn cycles_and_nested_no_gc_match_direct_runtime() {
    let mut cycle = Session::new();
    let r = cycle.create_region(CollectionMode::Manual, 1024).unwrap();
    let a = cycle.alloc_object(r, 0, 1).unwrap();
    cycle.root(a).unwrap();
    let b = cycle.alloc_object(r, 0, 1).unwrap();
    cycle.root(b).unwrap();
    cycle.set_reference(a, 0, Some(b)).unwrap();
    cycle.set_reference(b, 0, Some(a)).unwrap();
    cycle.unroot(b).unwrap();
    cycle.collect(r).unwrap();
    cycle
        .read_bytes(cycle.reference(a, 0).unwrap().unwrap())
        .unwrap();
    cycle.unroot(a).unwrap();
    cycle.collect(r).unwrap();
    cycle.close_region(r).unwrap();
    assert_region_parity(
        include_str!("fixtures/object_cycle.cg"),
        cycle.memory_report(),
    );

    let mut nested = Session::new();
    let r = nested.create_region(CollectionMode::Manual, 1024).unwrap();
    {
        let mut outer = nested.no_gc(256);
        let a = outer.alloc_object(r, 8, 1).unwrap();
        outer.root(a).unwrap();
        {
            let mut inner = outer.no_gc(128);
            let b = inner.alloc_object(r, 4, 0).unwrap();
            inner.root(b).unwrap();
            inner.set_reference(a, 0, Some(b)).unwrap();
            inner.unroot(b).unwrap();
        }
        outer
            .read_bytes(outer.reference(a, 0).unwrap().unwrap())
            .unwrap();
        outer.set_reference(a, 0, None).unwrap();
        outer.unroot(a).unwrap();
    }
    nested.collect(r).unwrap();
    nested.close_region(r).unwrap();
    assert_region_parity(
        include_str!("fixtures/object_nested_no_gc.cg"),
        nested.memory_report(),
    );
}

#[test]
fn stale_reference_and_budget_failure_keep_runtime_diagnostics() {
    for (source, code, message) in [
        (
            include_str!("fixtures/object_stale_reference.cg"),
            "CGR014",
            "InvalidHandle",
        ),
        (
            include_str!("fixtures/object_budget_failure.cg"),
            "CGR013",
            "BudgetExceeded",
        ),
    ] {
        check_source(source).unwrap();
        let errors = run_checked_source(source, &[]).unwrap_err();
        assert_eq!(errors[0].code, code);
        assert!(errors[0].message.contains(message), "{}", errors[0].message);
        let span = errors[0].span.unwrap();
        assert!(source[span.start..span.end].contains("memory."));
    }
    let empty = "let r = memory.region(mode: manual, limit: 1024)\nlet a = memory.alloc_object(r, 0, 1)\nlet missing = memory.reference(a, 0)\nmemory.use_reference(missing)\n";
    check_source(empty).unwrap();
    let errors = run_checked_source(empty, &[]).unwrap_err();
    assert_eq!(errors[0].code, "CGR014");
    assert!(errors[0].message.contains("empty object reference"));
}

#[test]
fn cli_check_run_json_fmt_and_failure_output_agree() {
    let root = env!("CARGO_MANIFEST_DIR");
    let success = format!("{root}/tests/fixtures/object_references.cg");
    let invalid = format!("{root}/tests/fixtures/object_invalid_slot.cg");
    let stale = format!("{root}/tests/fixtures/object_stale_reference.cg");
    for args in [
        vec!["check", &success],
        vec!["run", &success],
        vec!["fmt", &success],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["--diagnostic-format=json", "check", &invalid])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("CGM002"));
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["run", &stale])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("CGR014"));
}

#[test]
fn local_graph_function_runs_with_one_session() {
    let source = include_str!("fixtures/object_function.cg");
    let report = source_report(source);
    assert_eq!(report.regions.len(), 2);
    assert_eq!(report.regions[1].collection_count, 1);
    assert!(!report.regions[1].is_open);
}
