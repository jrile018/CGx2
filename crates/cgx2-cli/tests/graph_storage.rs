use cgx2_cli::{check_source, run_checked_source, run_checked_source_with_failure, Value};

const CHUNKS: &str = include_str!("../../cgx2-syntax/s2b-memory-evidence/chunked-cycle.cg");
const BYTES: &str = include_str!("../../cgx2-syntax/s2b-memory-evidence/initialized-object.cg");
const GUARDS: &str = include_str!("../../cgx2-syntax/s2b-memory-evidence/graph-no-gc.cg");

#[test]
fn real_source_graphs_reclaim_cycles_and_preserve_nested_session_guards() {
    for source in [CHUNKS, BYTES, GUARDS] {
        check_source(source).unwrap();
        let (result, report) = run_checked_source(source, &[]).unwrap();
        assert_eq!(result.value, Value::Unit);
        let report = report.unwrap();
        assert_eq!(report.totals.committed_bytes, 0);
        assert!(report.regions.iter().skip(1).all(|r| !r.is_open));
        if source == GUARDS {
            assert_eq!(report.regions.len(), 5);
            assert_eq!(report.totals.collection_count, 0);
            assert!(report.regions.iter().skip(1).all(|r| r.peak_bytes > 0));
        } else {
            assert!(report.totals.collection_count >= 2);
            assert!(report.totals.total_collected_bytes > 0);
        }
    }
}

#[test]
fn graph_and_payload_layout_errors_reject_at_real_source_spans() {
    let prefix = "# UTF-8 λ\nlet r = memory.region(mode: manual, limit: 8388608)\n";
    for body in [
        "let x = memory.alloc_graph(r, 0, 65537)\n",
        "let x = memory.alloc_object(r, 0, 257)\n",
        "let x = memory.alloc_graph(r, 1048577, 0)\n",
        "let x = memory.alloc_graph(r, 0, 257)\nmemory.set_reference(x, 0, x)\n",
        "let x = memory.alloc_object(r, 0, 1)\nmemory.set_graph_reference(x, 0, x)\n",
        "let x = memory.alloc_graph(r, 1, 1)\nmemory.write_graph_byte(x, 1, 0)\n",
        "let x = memory.alloc_graph(r, 1, 1)\nmemory.write_graph_byte(x, 0, 256)\n",
        "let x = memory.alloc_graph(r, 0, 1)\nmemory.set_graph_reference(x, 1, x)\n",
        "let x = memory.alloc_graph(r, 0, 1)\nlet s = memory.region(mode: manual, limit: 1024)\nlet y = memory.alloc_graph(s, 0, 0)\nmemory.set_graph_reference(x, 0, y)\n",
    ] {
        let source = format!("{prefix}{body}");
        let errors = check_source(&source).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "CGM002"), "{errors:?}");
        for error in errors {
            assert!(error.span.start >= prefix.len(), "{error:?}");
            assert!(source.get(error.span.start..error.span.end).is_some());
        }
    }
}

#[test]
fn source_failure_preserves_primary_error_and_releases_graph_roots() {
    for (source, expected) in [
        (
            include_str!("../../cgx2-syntax/s2b-memory-evidence/graph-budget-error.cg"),
            "CGR013",
        ),
        (
            include_str!("../../cgx2-syntax/s2b-memory-evidence/graph-byte-error.cg"),
            "CGR015",
        ),
    ] {
        check_source(source).unwrap();
        let errors = run_checked_source_with_failure(source, &[]).unwrap_err();
        assert_eq!(errors.len(), 1);
        let failure = &errors[0];
        assert_eq!(failure.error.code, expected);
        let span = failure.error.span.unwrap();
        assert!(source[span.start..span.end].contains("memory."));
        assert!(failure.cleanup_errors.is_empty());
        assert_eq!(
            failure
                .final_unwind
                .as_ref()
                .unwrap()
                .runtime_root_registrations,
            Some(0)
        );
        assert_eq!(failure.report.as_ref().unwrap().totals.collection_count, 0);
        if expected == "CGR015" {
            assert_eq!(
                failure.operation_failure.as_ref().unwrap().rooted_edges,
                Some(1)
            );
            assert_eq!(
                failure
                    .operation_failure
                    .as_ref()
                    .unwrap()
                    .runtime_root_registrations,
                Some(1)
            );
        }
    }
}

#[test]
fn session_guard_rejects_transitively_collecting_graph_callees() {
    let source = "fn collect() -> Unit effects(Collect, ManagedAlloc, Mutate) caps(MemoryCollect):\n    let r = memory.region(mode: manual, limit: 4096)\n    let x = memory.alloc_graph(r, 0, 1)\n    memory.drop(x)\n    memory.collect(r)\n    memory.close(r)\n\nfn main() -> Unit effects(Collect, ManagedAlloc, Mutate) caps(MemoryCollect):\n    no_gc(budget: 4096):\n        collect()\n";
    let errors = check_source(source).unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("collect")
            && source[e.span.start..e.span.end].contains("collect()")),
        "{errors:?}"
    );
}

#[cfg(feature = "failure-injection-test")]
#[test]
fn injected_source_failures_leave_no_graph_roots_or_manual_collections() {
    use cgx2_cli::run_checked_source_with_failure_at_reserve;
    let mut failures = 0;
    for reserve in 1..30 {
        match run_checked_source_with_failure_at_reserve(GUARDS, &[], reserve) {
            Ok((_, report)) => assert_eq!(report.unwrap().totals.collection_count, 0),
            Err(errors) => {
                failures += 1;
                let error = &errors[0];
                assert_eq!(error.error.code, "CGR013");
                assert_eq!(
                    error
                        .final_unwind
                        .as_ref()
                        .unwrap()
                        .runtime_root_registrations,
                    Some(0)
                );
                assert_eq!(error.report.as_ref().unwrap().totals.collection_count, 0);
                assert!(error.cleanup_errors.is_empty());
            }
        }
    }
    assert!(failures >= 10);
}
