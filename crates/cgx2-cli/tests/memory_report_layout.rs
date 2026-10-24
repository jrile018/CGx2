//! Source reporting regressions, not an ordinary-function memory capability claim.
use cgx2_cli::{check_source, run_checked_source, run_checked_source_with_failure, Value};

const CASES: &str = include_str!("memory_report_layout/expected.json");

#[test]
fn source_layout_census_preserves_exact_errors_values_and_cleanup() {
    let cases: serde_json::Value = serde_json::from_str(CASES).unwrap();
    for (name, expected) in cases.as_object().unwrap() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/memory_report_layout")
            .join(format!("{name}.cg"));
        let source = std::fs::read_to_string(path).unwrap();
        check_source(&source).unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        if expected["exit"] == 0 {
            let (value, report) = run_checked_source(&source, &[]).unwrap();
            assert_eq!(value.value, Value::Unit, "{name}");
            assert_eq!(report.unwrap().totals.committed_bytes, 0, "{name}");
            continue;
        }
        let plain = run_checked_source(&source, &[]).unwrap_err();
        let observed = run_checked_source_with_failure(&source, &[]).unwrap_err();
        assert_eq!(plain.len(), 1, "{name}");
        assert_eq!(observed.len(), 1, "{name}");
        let failure = &observed[0];
        assert_eq!(plain[0].code, failure.error.code, "{name}");
        assert_eq!(plain[0].message, failure.error.message, "{name}");
        assert_eq!(plain[0].span, failure.error.span, "{name}");
        assert_eq!(
            failure.error.code,
            expected["code"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            failure.error.message,
            expected["message"].as_str().unwrap(),
            "{name}"
        );
        let span = failure.error.span.unwrap();
        assert_eq!(
            span.start as u64,
            expected["span"]["start"].as_u64().unwrap(),
            "{name}"
        );
        assert_eq!(
            span.end as u64,
            expected["span"]["end"].as_u64().unwrap(),
            "{name}"
        );
        let operation = failure.operation_failure.as_ref().unwrap();
        assert_eq!(
            operation.rooted_handles.map(|n| n as u64),
            expected["rooted_handles"].as_u64(),
            "{name}"
        );
        assert_eq!(
            operation.rooted_edges.map(|n| n as u64),
            expected["rooted_edges"].as_u64(),
            "{name}"
        );
        assert_eq!(
            operation.runtime_rooted_handles, operation.rooted_handles,
            "{name}"
        );
        let cleanup = failure.final_unwind.as_ref().unwrap();
        assert_eq!(cleanup.rooted_handles, Some(0), "{name}");
        assert_eq!(cleanup.rooted_edges, Some(0), "{name}");
        assert_eq!(cleanup.runtime_rooted_handles, Some(0), "{name}");
        assert_eq!(cleanup.runtime_root_registrations, Some(0), "{name}");
        assert_eq!(failure.rollback_preserved, Some(true), "{name}");
        assert!(failure.cleanup_errors.is_empty(), "{name}");
    }
}

// Exercise the actual emitted helper template with runtime-created optional
// child aliases. The source profile only admits use_reference on these optional
// values; it has no optional-to-managed conversion or reference function result.
// Match the existing generated support's arity/tuple contracts without changing
// the generator owned by the separate guard repair lane.
#[allow(dead_code, clippy::type_complexity, clippy::too_many_arguments)]
mod native_template {
    include!("../../cgx2-emit-rust/src/memory_runtime.rs.txt");
    fn cgx2_json_string(value: &str) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn returned_alias_keeps_child_layout_and_missing_provenance_is_unknown() {
        CGX2_OBSERVE.with(|value| value.set(true));
        let mut session = Session::new();
        let mut bindings = HashMap::new();
        open(
            &mut session,
            &mut bindings,
            "r",
            CollectionMode::Manual,
            8 << 20,
            (0, 0),
        )
        .unwrap();
        alloc_object(&mut session, &mut bindings, "raw", "r", 24, 1, (0, 0)).unwrap();
        alloc_graph(&mut session, &mut bindings, "graph", "r", 0, 65536, (0, 0)).unwrap();
        set_reference(&mut session, &bindings, "raw", 0, Some("graph"), (0, 0)).unwrap();
        set_graph_reference(&mut session, &bindings, "graph", 65535, Some("raw"), (0, 0)).unwrap();
        for (owner, graph, index, alias) in [
            ("raw", false, 0, "to_graph"),
            ("graph", true, 65535, "to_raw"),
        ] {
            if graph {
                graph_reference(&session, &mut bindings, alias, owner, index, (0, 0)).unwrap();
            } else {
                reference(&session, &mut bindings, alias, owner, index, (0, 0)).unwrap();
            }
            let Binding::Reference(Some(child)) = bindings[alias] else {
                panic!("missing alias")
            };
            // Inspect the returned child alone, after its original binding is gone.
            let aliases = HashMap::from([
                ("alias", Binding::Managed(child)),
                ("duplicate", Binding::Managed(child)),
            ]);
            let snapshot = graph_snapshot(&session, &aliases).unwrap();
            assert_eq!(snapshot[0].3.iter().flatten().count(), 1);
            let mut observation = FailureObservation::new(&session);
            observation.add_roots(&session, &aliases);
            let census: serde_json::Value = serde_json::from_str(&observation.to_json()).unwrap();
            assert_eq!(census["rooted_handles"], 1);
            assert_eq!(census["rooted_edges"], 1);
            let layout = CGX2_LAYOUTS.with(|layouts| layouts.borrow_mut().remove(&child).unwrap());
            assert!(graph_snapshot(&session, &aliases).is_none());
            observation.add_roots(&session, &aliases);
            let unknown: serde_json::Value = serde_json::from_str(&observation.to_json()).unwrap();
            assert!(unknown["rooted_handles"].is_null());
            assert!(unknown["rooted_edges"].is_null());
            remember_layout(child, layout);
        }
        // Mutating an explicit graph's header must still fail layout validation.
        let graph = managed(&bindings, "graph", (0, 0), "owner").unwrap();
        session.write_bytes(graph, 0, &[0]).unwrap();
        assert!(graph_snapshot(&session, &bindings).is_none());
        assert_eq!(session.read_bytes(graph).unwrap()[0], 0);
    }
}
