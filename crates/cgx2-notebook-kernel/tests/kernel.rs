use cgx2_notebook_format::{
    make_provenance, parse_notebook, write_notebook, Cell, CellKind, Notebook, Output,
};
use cgx2_notebook_kernel::{
    facts_for_source, ExecutionControl, NotebookKernel, MAX_CODE_CELLS, MAX_SOURCE_BYTES,
};
use cgx2_notebook_session::CellStatus;
use std::time::{Duration, Instant};

fn code(id: &str, source: &str) -> Cell {
    Cell {
        id: id.into(),
        kind: CellKind::Code,
        source: source.into(),
        outputs: vec![],
        provenance: None,
    }
}

fn notebook(cells: Vec<Cell>) -> Notebook {
    Notebook { version: 1, cells }
}

fn output_body(output: &Output) -> &str {
    match output {
        Output::Text { body, .. } => body,
        Output::Asset { .. } => panic!("expected text output"),
    }
}

#[test]
fn deleting_a_function_cell_preserves_independent_output_across_reopen() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("base", "fn inc(x: i64) -> i64:\n    x + 1\n"),
        code("derived", "fn twice(x: i64) -> i64:\n    inc(inc(x))\n"),
        code("value", "twice(3)"),
        code("independent", "40 + 2"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    let independent_revision = kernel.notebook().cells[3]
        .provenance
        .as_ref()
        .unwrap()
        .ordinal;
    let old_value = kernel.notebook().cells[2].outputs.clone();

    kernel.remove_cell("base").unwrap();
    assert_eq!(kernel.status("derived").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("value").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
    assert_eq!(kernel.notebook().cells[1].outputs, old_value);
    assert_eq!(
        kernel.notebook().cells[2]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        independent_revision
    );

    let saved = write_notebook(kernel.notebook()).unwrap();
    let mut reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("value").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
    assert_eq!(reopened.rerun_cell("derived").unwrap_err().code, "CGNK011");
    let failed_saved = write_notebook(reopened.notebook()).unwrap();
    let failed_reopen = NotebookKernel::open(parse_notebook(&failed_saved).unwrap()).unwrap();
    assert_eq!(failed_reopen.status("derived").unwrap(), CellStatus::Failed);
    assert_eq!(failed_reopen.status("value").unwrap(), CellStatus::Stale);
    assert_eq!(
        failed_reopen.status("independent").unwrap(),
        CellStatus::Cached
    );
    reopened.restart().unwrap();
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.rerun_cell("derived").unwrap_err().code, "CGNK011");
    assert_eq!(reopened.status("value").unwrap(), CellStatus::Stale);
}

#[test]
fn interrupted_or_invalid_definition_never_publishes_a_function() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("defs", "fn inc(x: i64) -> i64:\n    x + 1\n"),
        code("value", "inc(2)"),
    ]))
    .unwrap();
    let control = ExecutionControl::new(
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
        Duration::from_secs(2),
    );
    kernel.set_execution_control(control.clone());
    control.interrupt();
    assert_eq!(kernel.run_cell("defs").unwrap_err().code, "CGNK010");
    assert_eq!(kernel.status("defs").unwrap(), CellStatus::Failed);
    assert_eq!(kernel.rerun_cell("value").unwrap_err().code, "CGNK004");
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "3");

    kernel
        .replace_cell(
            "defs",
            "fn inc(x: i64) -> i64:\n    x + 2\n\nfn inc(y: i64) -> i64:\n    y + 3\n".into(),
        )
        .unwrap();
    assert_eq!(kernel.rerun_affected("defs").unwrap_err().code, "CGNK001");
    assert_eq!(kernel.status("value").unwrap(), CellStatus::Stale);
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "3");
    kernel
        .replace_cell("defs", "fn inc(x: i64) -> i64:\n    x + 2\n".into())
        .unwrap();
    kernel.rerun_affected("defs").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "4");
}

#[test]
fn reopen_preserves_observation_and_prefix_edit_is_stale() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("seed", "let x = 3"),
        code("value", "x + 2"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    assert!(kernel
        .notebook()
        .cells
        .iter()
        .all(|cell| cell.provenance.is_some()));
    let saved = write_notebook(kernel.notebook()).unwrap();
    let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("seed").unwrap(), CellStatus::Cached);
    assert_eq!(reopened.status("value").unwrap(), CellStatus::Cached);
    assert_eq!(output_body(&reopened.notebook().cells[1].outputs[0]), "5");
    let mut edited = parse_notebook(&saved).unwrap();
    edited.cells[0].source = "let x = 4".into();
    let stale =
        NotebookKernel::open(parse_notebook(&write_notebook(&edited).unwrap()).unwrap()).unwrap();
    assert_eq!(stale.status("seed").unwrap(), CellStatus::Stale);
    assert_eq!(stale.status("value").unwrap(), CellStatus::Stale);
}

#[test]
fn repeated_runs_advance_execution_and_legacy_output_ordinals() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("silent", "let x = 3"),
        code("value", "x"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(
        kernel.notebook().cells[0]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        1
    );
    assert_eq!(
        kernel.notebook().cells[1]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        2
    );
    assert_eq!(
        kernel.notebook().cells[1]
            .provenance
            .as_ref()
            .unwrap()
            .output_ordinal,
        Some(1)
    );
    kernel.rerun_cell("value").unwrap();
    assert_eq!(
        kernel.notebook().cells[1]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        3
    );
    assert_eq!(
        kernel.notebook().cells[1]
            .provenance
            .as_ref()
            .unwrap()
            .output_ordinal,
        Some(2)
    );
    assert_eq!(kernel.status("silent").unwrap(), CellStatus::Current);
}

#[test]
fn sequential_cells_share_scalar_binding_and_roundtrip() {
    let notebook = parse_notebook(include_str!("fixtures/scalar.cgnb")).unwrap();
    let mut kernel = NotebookKernel::open(notebook).unwrap();
    kernel.run_cell("seed").unwrap();
    kernel.run_cell("result").unwrap();
    let cells = &kernel.notebook().cells;
    assert_eq!(cells[0].source, "# Scalar notebook\n");
    assert!(cells[0].outputs.is_empty());
    assert!(cells[1].outputs.is_empty());
    assert_eq!(
        cells[2].outputs,
        vec![Output::Text {
            mime: "text/plain".into(),
            ordinal: 1,
            body: "7".into()
        }]
    );
    let reopened = parse_notebook(&write_notebook(kernel.notebook()).unwrap()).unwrap();
    assert_eq!(reopened, *kernel.notebook());
}

#[test]
fn facts_ignore_escaped_string_contents_and_collect_expression_reads() {
    let facts = facts_for_source("let answer = 3\n\"answer\\n\"\nanswer + 4").unwrap();
    assert_eq!(
        facts.defines.into_iter().collect::<Vec<_>>(),
        vec!["answer"]
    );
    assert!(facts.reads.is_empty());
    assert!(!facts.opaque_effects);
    let shadow = facts_for_source("let answer = answer + 1").unwrap();
    assert_eq!(shadow.reads.into_iter().collect::<Vec<_>>(), vec!["answer"]);
}

#[test]
fn failed_cell_does_not_enter_replay_prefix() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("seed", "let x = 3\nx"),
        code("later", "missing + 4"),
    ]))
    .unwrap();
    kernel.run_cell("seed").unwrap();
    let error = kernel.run_cell("later").unwrap_err();
    assert_eq!(error.code, "CGNK008");
    assert_eq!(kernel.status("later").unwrap(), CellStatus::Failed);
    assert!(kernel.notebook().cells[1].outputs.is_empty());
    assert_eq!(
        kernel.notebook().cells[0].outputs[0],
        Output::Text {
            mime: "text/plain".into(),
            ordinal: 1,
            body: "3".into()
        }
    );
    kernel.replace_cell("later", "x + 4".into()).unwrap();
    assert_eq!(kernel.run_cell("later").unwrap_err().code, "CGNK004");
    kernel.restart_and_run_all().unwrap();
    assert_eq!(
        kernel.notebook().cells[1].outputs[0],
        Output::Text {
            mime: "text/plain".into(),
            ordinal: 3,
            body: "7".into()
        }
    );
}

#[test]
fn edit_stales_transitive_readers_and_restart_recomputes() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("a", "let x = 3"),
        code("b", "let y = x + 4"),
        code("c", "y + 1"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(kernel.status("c").unwrap(), CellStatus::Current);
    kernel.replace_cell("a", "let x = 5".into()).unwrap();
    assert_eq!(kernel.status("b").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("c").unwrap(), CellStatus::Stale);
    kernel.restart_and_run_all().unwrap();
    assert_eq!(
        kernel.notebook().cells[2].outputs[0],
        Output::Text {
            mime: "text/plain".into(),
            ordinal: 2,
            body: "10".into()
        }
    );
}

#[test]
fn restart_failure_retains_stale_old_outputs() {
    let mut kernel =
        NotebookKernel::open(notebook(vec![code("a", "let x = 3"), code("b", "x + 4")])).unwrap();
    kernel.restart_and_run_all().unwrap();
    kernel.replace_cell("a", "let x = missing".into()).unwrap();
    assert_eq!(kernel.restart_and_run_all().unwrap_err().code, "CGNK008");
    assert_eq!(kernel.status("a").unwrap(), CellStatus::Failed);
    assert_eq!(kernel.status("b").unwrap(), CellStatus::Stale);
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "7");
}

#[test]
fn restart_marks_saved_observations_stale_and_reopen_never_executes() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("seed", "let x = 3\nx"),
        code("result", "x + 4"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    let old_revision = kernel.notebook().cells[1]
        .provenance
        .as_ref()
        .unwrap()
        .ordinal;
    kernel.restart().unwrap();
    assert_eq!(kernel.status("seed").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("result").unwrap(), CellStatus::Stale);
    let saved = write_notebook(kernel.notebook()).unwrap();
    let mut reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("result").unwrap(), CellStatus::Stale);
    assert_eq!(output_body(&reopened.notebook().cells[1].outputs[0]), "7");
    assert_eq!(
        reopened.notebook().cells[1]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        old_revision
    );
    assert_eq!(reopened.run_cell("result").unwrap_err().code, "CGNK005");
    reopened.run_cell("seed").unwrap();
    reopened.run_cell("result").unwrap();
    assert_eq!(reopened.status("result").unwrap(), CellStatus::Current);
}

#[cfg(unix)]
#[test]
fn stalled_worker_is_killed_and_later_rerun_has_clean_bindings() {
    use std::os::unix::fs::PermissionsExt;
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("seed", "let x = 3"),
        code("value", "let y = x + 4\ny"),
        code("reader", "y + 1"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    let seed_revision = kernel.notebook().cells[0]
        .provenance
        .as_ref()
        .unwrap()
        .ordinal;
    let seed_output = kernel.notebook().cells[0].outputs.clone();
    let temp = std::env::temp_dir().join(format!("cgx2-notebook-sleep-{}", std::process::id()));
    std::fs::write(&temp, "#!/bin/sh\nexec sleep 2\n").unwrap();
    std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o700)).unwrap();
    kernel.set_execution_control(ExecutionControl::new(
        temp.clone(),
        Duration::from_millis(25),
    ));
    let start = Instant::now();
    let error = kernel.rerun_cell("value").unwrap_err();
    let _ = std::fs::remove_file(temp);
    assert_eq!(error.code, "CGNK010");
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(kernel.status("value").unwrap(), CellStatus::Failed);
    assert_eq!(kernel.status("reader").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("seed").unwrap(), CellStatus::Current);
    let saved = write_notebook(kernel.notebook()).unwrap();
    let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("value").unwrap(), CellStatus::Failed);
    assert_eq!(reopened.status("reader").unwrap(), CellStatus::Stale);
    assert_eq!(output_body(&reopened.notebook().cells[2].outputs[0]), "8");
    assert_eq!(
        kernel.notebook().cells[0]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        seed_revision
    );
    assert_eq!(kernel.notebook().cells[0].outputs, seed_output);
    kernel.set_execution_control(ExecutionControl::new(
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
        Duration::from_secs(2),
    ));
    kernel.rerun_cell("value").unwrap();
    kernel.rerun_cell("reader").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "8");
}

#[test]
fn rerun_and_out_of_order_are_explicit_errors() {
    let mut kernel =
        NotebookKernel::open(notebook(vec![code("a", "let x = 3"), code("b", "x + 4")])).unwrap();
    assert_eq!(kernel.run_cell("b").unwrap_err().code, "CGNK005");
    kernel.run_cell("a").unwrap();
    assert_eq!(kernel.run_cell("a").unwrap_err().code, "CGNK004");
    kernel.run_cell("b").unwrap();
}

#[test]
fn duplicate_definition_and_unsupported_forms_are_rejected() {
    let mut duplicate = NotebookKernel::open(notebook(vec![
        code("a", "let x = 3"),
        code("b", "let x = 4"),
    ]))
    .unwrap();
    duplicate.run_cell("a").unwrap();
    assert_eq!(duplicate.run_cell("b").unwrap_err().code, "CGNK008");
    for source in [
        "for x in xs:\n    x",
        "arg_count()",
        "let mut x = 3",
        "x = 4",
    ] {
        let mut kernel = NotebookKernel::open(notebook(vec![code("a", source)])).unwrap();
        let expected = if source == "arg_count()" {
            "CGNK011"
        } else {
            "CGNK001"
        };
        assert_eq!(kernel.run_cell("a").unwrap_err().code, expected, "{source}");
    }
}

#[test]
fn pure_functions_keep_live_definitions_and_selective_revisions() {
    let source = include_str!("fixtures/pure-functions.cgnb");
    let mut kernel = NotebookKernel::open(parse_notebook(source).unwrap()).unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "5");
    let independent = kernel.notebook().cells[2]
        .provenance
        .as_ref()
        .unwrap()
        .ordinal;
    let saved = write_notebook(kernel.notebook()).unwrap();
    let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("defs").unwrap(), CellStatus::Cached);
    assert_eq!(reopened.status("value").unwrap(), CellStatus::Cached);

    kernel
        .replace_cell(
            "defs",
            "fn inc(x: i64) -> i64:\n    x + 2\n\nfn twice(x: i64) -> i64:\n    inc(inc(x))\n"
                .into(),
        )
        .unwrap();
    assert_eq!(kernel.status("value").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
    kernel.rerun_affected("defs").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "7");
    assert_eq!(
        kernel.notebook().cells[2]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        independent
    );

    kernel
        .replace_cell("defs", "fn inc(x: i64) -> i64:\n    x + 2\n".into())
        .unwrap();
    assert_eq!(kernel.status("value").unwrap(), CellStatus::Stale);
    let error = kernel.rerun_affected("defs").unwrap_err();
    assert_eq!(error.code, "CGNK011");
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
    let saved = write_notebook(kernel.notebook()).unwrap();
    let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("value").unwrap(), CellStatus::Failed);
}

#[test]
fn changed_function_stales_transitive_function_callers() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("base", "fn inc(x: i64) -> i64:\n    x + 1\n"),
        code("derived", "fn twice(x: i64) -> i64:\n    inc(inc(x))\n"),
        code("value", "twice(3)"),
        code("independent", "40 + 2"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "5");
    let independent = kernel.notebook().cells[3]
        .provenance
        .as_ref()
        .unwrap()
        .ordinal;
    kernel
        .replace_cell("base", "fn inc(x: i64) -> i64:\n    x + 2\n".into())
        .unwrap();
    assert_eq!(kernel.status("derived").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("value").unwrap(), CellStatus::Stale);
    kernel.rerun_affected("base").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "7");
    assert_eq!(
        kernel.notebook().cells[3]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        independent
    );
}

#[test]
fn pure_bool_and_string_signatures_use_cli_scalar_semantics() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code(
            "defs",
            "fn echo_flag(x: Bool) -> Bool:\n    x\n\nfn greet(x: String) -> String:\n    x\n",
        ),
        code("bool", "echo_flag(true)"),
        code("string", "greet(\"hello\")"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "true");
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "hello");
}

#[test]
fn pure_functions_reject_effects_capture_recursion_and_main() {
    for source in [
        "fn main() -> i64:\n    1\n",
        "fn f() -> i64 effects(Foreign):\n    1\n",
        "foreign python fn f() -> i64 effects(Foreign) caps(ForeignCall) = math.isqrt runtime \"local:cpython\" ownership copied\n",
        "fn f() -> i64:\n    f()\n",
        "fn f() -> i64:\n    x\n",
    ] {
        let mut kernel = NotebookKernel::open(notebook(vec![code("a", "let x = 2"), code("b", source)])).unwrap();
        kernel.run_cell("a").unwrap();
        let error = kernel.run_cell("b").unwrap_err();
        assert!(matches!(error.code, "CGNK011" | "CGNK001"), "{source}: {error}");
    }
}

#[cfg(unix)]
#[test]
fn interrupt_during_pure_call_leaves_failed_observation() {
    use std::os::unix::fs::PermissionsExt;
    let source = include_str!("fixtures/pure-functions.cgnb");
    let mut kernel = NotebookKernel::open(parse_notebook(source).unwrap()).unwrap();
    kernel.run_cell("defs").unwrap();
    let worker = std::env::temp_dir().join(format!("cgx2-pure-call-worker-{}", std::process::id()));
    let marker = std::env::temp_dir().join(format!("cgx2-pure-call-marker-{}", std::process::id()));
    std::fs::write(
        &worker,
        format!("#!/bin/sh\ntouch {}\nexec sleep 2\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o700)).unwrap();
    let control = ExecutionControl::new(worker.clone(), Duration::from_secs(2));
    kernel.set_execution_control(control.clone());
    let running = std::thread::spawn(move || {
        let result = kernel.run_cell("value");
        (kernel, result)
    });
    let started = Instant::now();
    while !marker.exists() && started.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(marker.exists(), "pure call did not reach the worker");
    control.interrupt();
    let (mut kernel, result) = running.join().unwrap();
    let _ = std::fs::remove_file(worker);
    let _ = std::fs::remove_file(marker);
    assert_eq!(result.unwrap_err().code, "CGNK010");
    assert_eq!(kernel.status("value").unwrap(), CellStatus::Failed);
    kernel.set_execution_control(ExecutionControl::new(
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
        Duration::from_secs(2),
    ));
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "5");
}

#[test]
fn calls_with_file_network_or_foreign_names_fail_before_execution() {
    for source in [
        "read_file(\"secret\")",
        "network_get(\"example\")",
        "foreign_call()",
    ] {
        let mut kernel = NotebookKernel::open(notebook(vec![code("effect", source)])).unwrap();
        let error = kernel.run_cell("effect").unwrap_err();
        assert_eq!(error.code, "CGNK011", "{source}");
        assert!(error.message.contains("file, network, and foreign effects"));
        assert!(kernel.notebook().cells[0].outputs.is_empty());
        assert_eq!(kernel.status("effect").unwrap(), CellStatus::Failed);
    }
}

#[test]
fn saved_unsupported_effect_and_later_output_reopen_stale_without_execution() {
    let mut saved = notebook(vec![
        code("before", "1"),
        code("effect", "foreign_call()"),
        code("later", "2"),
    ]);
    for index in 0..saved.cells.len() {
        let ordinal = (index + 1) as u64;
        saved.cells[index].outputs.push(Output::Text {
            mime: "text/plain".into(),
            ordinal,
            body: format!("old-{index}"),
        });
        saved.cells[index].provenance =
            Some(make_provenance(&saved, index, ordinal, Some(ordinal)));
    }
    let text = write_notebook(&saved).unwrap();
    let reopened = NotebookKernel::open(parse_notebook(&text).unwrap()).unwrap();
    assert_eq!(reopened.status("before").unwrap(), CellStatus::Cached);
    assert_eq!(reopened.status("effect").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("later").unwrap(), CellStatus::Stale);
    assert_eq!(
        output_body(&reopened.notebook().cells[2].outputs[0]),
        "old-2"
    );
}

#[test]
fn effect_boundary_fixture_tracks_edit_interrupt_restart_and_passive_reopen() {
    let source = include_str!("fixtures/effect-boundary.cgnb");
    let mut kernel = NotebookKernel::open(parse_notebook(source).unwrap()).unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(kernel.notebook().cells[0].kind, CellKind::Markdown);
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "7");
    let independent_revision = kernel.notebook().cells[4]
        .provenance
        .as_ref()
        .unwrap()
        .ordinal;

    kernel.replace_cell("seed", "let x = 5".into()).unwrap();
    assert_eq!(kernel.status("seed").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("reader").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
    kernel.rerun_cell("seed").unwrap();

    let control = ExecutionControl::new(
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
        Duration::from_secs(2),
    );
    kernel.set_execution_control(control.clone());
    control.interrupt();
    assert_eq!(kernel.rerun_cell("reader").unwrap_err().code, "CGNK010");
    assert_eq!(kernel.status("reader").unwrap(), CellStatus::Failed);
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "7");
    assert_eq!(
        kernel.notebook().cells[4]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        independent_revision
    );

    let interrupted = write_notebook(kernel.notebook()).unwrap();
    let mut reopened = NotebookKernel::open(parse_notebook(&interrupted).unwrap()).unwrap();
    assert_eq!(reopened.status("reader").unwrap(), CellStatus::Failed);
    assert_eq!(output_body(&reopened.notebook().cells[2].outputs[0]), "7");
    assert_eq!(reopened.run_cell("reader").unwrap_err().code, "CGNK005");

    kernel.restart_and_run_all().unwrap();
    assert_eq!(kernel.status("reader").unwrap(), CellStatus::Current);
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "9");
    let saved = write_notebook(kernel.notebook()).unwrap();
    let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("reader").unwrap(), CellStatus::Cached);
    assert_eq!(output_body(&reopened.notebook().cells[2].outputs[0]), "9");
}

#[test]
fn notebook_limits_are_checked_before_execution() {
    let cells = (0..=MAX_CODE_CELLS)
        .map(|n| code(&format!("c{n}"), "1"))
        .collect();
    assert_eq!(
        NotebookKernel::open(notebook(cells)).unwrap_err().code,
        "CGNK002"
    );
    let source = " ".repeat(MAX_SOURCE_BYTES + 1);
    assert_eq!(
        NotebookKernel::open(notebook(vec![code("a", &source)]))
            .unwrap_err()
            .code,
        "CGNK003"
    );
}

#[test]
fn escaped_string_is_evaluated_by_cgx2() {
    let mut kernel = NotebookKernel::open(notebook(vec![code("a", "\"line\\nnext\"")])).unwrap();
    kernel.run_cell("a").unwrap();
    assert_eq!(
        kernel.notebook().cells[0].outputs[0],
        Output::Text {
            mime: "text/plain".into(),
            ordinal: 1,
            body: "line\nnext".into()
        }
    );
}

#[test]
fn evaluation_budget_stops_a_long_sequence() {
    let source = format!("\"{}\"", "x".repeat(498));
    let cells = (0..120)
        .map(|n| code(&format!("c{n}"), &format!("let x{n} = {source}")))
        .collect();
    let mut kernel = NotebookKernel::open(notebook(cells)).unwrap();
    let error = kernel.restart_and_run_all().unwrap_err();
    assert_eq!(error.code, "CGNK006");
    assert!(error.cell_id.is_some());
    assert!(kernel
        .notebook()
        .cells
        .iter()
        .any(|cell| cell.outputs.is_empty()));
}

#[test]
fn selected_rerun_uses_retained_prefix_and_stales_transitive_readers() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("first", "let x = 3\nx"),
        code("middle", "let y = x + 4\ny"),
        code("last", "y + 1"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    kernel.replace_cell("first", "let x = 5\nx".into()).unwrap();
    kernel.rerun_cell("first").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[0].outputs[0]), "5");
    assert_eq!(kernel.status("middle").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("last").unwrap(), CellStatus::Stale);
    kernel.rerun_cell("middle").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "9");
    assert_eq!(kernel.status("last").unwrap(), CellStatus::Stale);
    kernel.rerun_cell("last").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "10");
    assert_eq!(kernel.status("first").unwrap(), CellStatus::Current);
    assert_eq!(kernel.status("middle").unwrap(), CellStatus::Current);
    assert_eq!(kernel.status("last").unwrap(), CellStatus::Current);
    kernel.rerun_cell("middle").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "9");
    assert_eq!(kernel.status("last").unwrap(), CellStatus::Stale);
}

#[test]
fn failed_rerun_never_leaves_old_result_current() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("first", "let x = 3"),
        code("last", "x + 4"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    kernel.replace_cell("last", "missing + 4".into()).unwrap();
    assert_eq!(kernel.rerun_cell("last").unwrap_err().code, "CGNK008");
    assert_eq!(kernel.status("last").unwrap(), CellStatus::Failed);
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "7");
    kernel.replace_cell("last", "x + 5".into()).unwrap();
    kernel.rerun_cell("last").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "8");
    assert_eq!(kernel.status("last").unwrap(), CellStatus::Current);
}

#[test]
fn rerun_evaluation_budget_is_cumulative_and_failure_is_noncurrent() {
    let source = format!("\"{}\"", "x".repeat(30_000));
    let mut kernel = NotebookKernel::open(notebook(vec![code("c", &source)])).unwrap();
    kernel.restart_and_run_all().unwrap();
    let mut error = None;
    for _ in 0..40 {
        if let Err(failure) = kernel.rerun_cell("c") {
            error = Some(failure);
            break;
        }
    }
    let error = error.unwrap();
    assert_eq!(error.code, "CGNK006");
    assert!(error.message.contains("evaluation budget"));
    assert_eq!(kernel.status("c").unwrap(), CellStatus::Failed);
}

#[test]
fn edited_prerequisite_requires_explicit_rerun_and_preserves_old_cache() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("a", "let x = 3"),
        code("b", "let y = x + 4"),
        code("c", "y + 1"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    kernel.replace_cell("a", "let x = 5".into()).unwrap();
    let error = kernel.rerun_cell("c").unwrap_err();
    assert_eq!(error.code, "CGNK004");
    assert_eq!(kernel.status("c").unwrap(), CellStatus::Stale);
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "8");
    kernel.rerun_cell("a").unwrap();
    kernel.rerun_cell("b").unwrap();
    kernel.rerun_cell("c").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "10");
    assert_eq!(kernel.status("c").unwrap(), CellStatus::Current);
}

#[test]
fn failing_cell_does_not_commit_partially_checked_bindings() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("seed", "let x = 3"),
        code("value", "let y = x + 4\n1 / 0"),
    ]))
    .unwrap();
    kernel.run_cell("seed").unwrap();
    let error = kernel.run_cell("value").unwrap_err();
    assert_eq!(error.code, "CGNK008");
    assert!(error.message.contains("CGR006"));
    kernel.replace_cell("value", "y".into()).unwrap();
    let error = kernel.rerun_cell("value").unwrap_err();
    assert_eq!(error.code, "CGNK008");
    assert!(error.message.contains("unknown name"));
    kernel.replace_cell("value", "x + 4".into()).unwrap();
    kernel.rerun_cell("value").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "7");
}

#[test]
fn retained_scalars_keep_cli_arithmetic_and_string_behavior() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("minimum", "let min = 0 - 9223372036854775807 - 1"),
        code("text", "let message: String = \"line\\n\\\"quoted\\\"\""),
        code("read", "message"),
        code("edge", "min"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(
        output_body(&kernel.notebook().cells[2].outputs[0]),
        "line\n\"quoted\""
    );
    assert_eq!(
        output_body(&kernel.notebook().cells[3].outputs[0]),
        i64::MIN.to_string()
    );
}

#[test]
fn selective_edit_reruns_only_transitive_reader_and_reopens_with_dependency_digest() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("seed", "let x = 3"),
        code("independent", "let z = 9"),
        code("result", "x + 4"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    let independent_revision = kernel.notebook().cells[1]
        .provenance
        .as_ref()
        .unwrap()
        .ordinal;
    let independent_digest = kernel.notebook().cells[1]
        .provenance
        .as_ref()
        .unwrap()
        .prefix_sha256
        .clone();

    kernel.replace_cell("seed", "let x = 5".into()).unwrap();
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
    assert_eq!(kernel.status("result").unwrap(), CellStatus::Stale);
    kernel.rerun_affected("seed").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "9");
    assert_eq!(
        kernel.notebook().cells[1]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        independent_revision
    );
    assert_eq!(
        kernel.notebook().cells[1]
            .provenance
            .as_ref()
            .unwrap()
            .prefix_sha256,
        independent_digest
    );

    let saved = write_notebook(kernel.notebook()).unwrap();
    let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
    assert_eq!(reopened.status("result").unwrap(), CellStatus::Cached);
    let mut edited = parse_notebook(&saved).unwrap();
    edited.cells[0].source = "let x = 6".into();
    let stale = NotebookKernel::open(edited).unwrap();
    assert_eq!(stale.status("independent").unwrap(), CellStatus::Cached);
    assert_eq!(stale.status("result").unwrap(), CellStatus::Stale);
}

#[test]
fn forward_dependency_cycle_is_reported_before_execution() {
    let error = NotebookKernel::open(notebook(vec![
        code("a", "let x = y"),
        code("b", "let y = x"),
    ]))
    .err()
    .unwrap();
    assert_eq!(error.code, "CGNK009");
    assert!(error.message.contains("dependency cycles"));
}

#[test]
fn rerun_affected_leaves_an_independent_edited_branch_stale() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("a", "let x = 1"),
        code("b", "let y = 2"),
        code("read_a", "x + 1"),
        code("read_b", "y + 1"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    let b_revision = kernel.notebook().cells[1]
        .provenance
        .as_ref()
        .unwrap()
        .ordinal;
    kernel.replace_cell("a", "let x = 5".into()).unwrap();
    kernel.replace_cell("b", "let y = 7".into()).unwrap();
    kernel.rerun_affected("a").unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "6");
    assert_eq!(kernel.status("b").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("read_b").unwrap(), CellStatus::Stale);
    assert_eq!(
        kernel.notebook().cells[1]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        b_revision
    );
}

#[test]
fn retained_records_cross_cells_and_reopen_cleanly() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("point", "record Point:\n    x: i64\n    active: Bool\n"),
        code("flag", "record Flag:\n    ok: Bool\n"),
        code(
            "value",
            "let p = Point(active: true, x: -9223372036854775807 - 1)\nlet f = Flag(ok: p.active)",
        ),
        code("read", "p.x"),
        code("bool", "f.ok"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(
        output_body(&kernel.notebook().cells[3].outputs[0]),
        "-9223372036854775808"
    );
    assert_eq!(output_body(&kernel.notebook().cells[4].outputs[0]), "true");
    let saved = write_notebook(kernel.notebook()).unwrap();
    let mut reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("read").unwrap(), CellStatus::Cached);
    reopened.restart_and_run_all().unwrap();
    assert_eq!(
        output_body(&reopened.notebook().cells[3].outputs[0]),
        "-9223372036854775808"
    );
    assert_eq!(
        output_body(&reopened.notebook().cells[4].outputs[0]),
        "true"
    );
}

#[test]
fn unicode_record_names_survive_worker_and_retained_replay() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("definition", "record 座標:\n    位置: i64\n"),
        code("value", "let p = 座標(位置: 7)"),
        code("read", "p.位置"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[2].outputs[0]), "7");
    let saved = write_notebook(kernel.notebook()).unwrap();
    let mut reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    reopened.restart_and_run_all().unwrap();
    assert_eq!(output_body(&reopened.notebook().cells[2].outputs[0]), "7");
}

#[test]
fn record_definition_edit_stales_dependents_and_reruns_selected_cells() {
    let mut kernel =
        NotebookKernel::open(parse_notebook(include_str!("fixtures/pure-records.cgnb")).unwrap())
            .unwrap();
    kernel.restart_and_run_all().unwrap();
    kernel
        .replace_cell(
            "point",
            "record Point:\n    x: i64\n    active: Bool\n    extra: i64\n".into(),
        )
        .unwrap();
    assert_eq!(kernel.status("values").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("read").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("bool").unwrap(), CellStatus::Stale);
    assert!(
        kernel.notebook().cells[3]
            .provenance
            .as_ref()
            .unwrap()
            .stale
    );
    kernel.rerun_cell("point").unwrap();
    let failure = kernel.rerun_cell("values").unwrap_err();
    assert_eq!(failure.cell_id.as_deref(), Some("values"));
    assert_eq!(kernel.status("values").unwrap(), CellStatus::Failed);
    kernel
        .replace_cell(
            "point",
            "record Point:\n    x: i64\n    active: Bool\n".into(),
        )
        .unwrap();
    kernel.rerun_cell("point").unwrap();
    kernel.rerun_cell("values").unwrap();
    kernel.rerun_cell("read").unwrap();
    kernel.rerun_cell("bool").unwrap();
    assert_eq!(kernel.status("read").unwrap(), CellStatus::Current);
    assert_eq!(
        output_body(&kernel.notebook().cells[3].outputs[0]),
        "-9223372036854775808"
    );
}

#[test]
fn record_worker_and_failure_preserve_valid_earlier_cells() {
    let mut document = parse_notebook(include_str!("fixtures/pure-records.cgnb")).unwrap();
    document
        .cells
        .push(code("bad", "Point(x: 1, active: true, missing: 2)"));
    let mut kernel = NotebookKernel::open(document).unwrap();
    kernel.set_execution_control(ExecutionControl::new(
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
        Duration::from_secs(2),
    ));
    for id in ["point", "flag", "values", "read", "bool"] {
        kernel.run_cell(id).unwrap();
    }
    assert_eq!(output_body(&kernel.notebook().cells[4].outputs[0]), "true");
    let failure = kernel.rerun_cell("bad").unwrap_err();
    assert_eq!(failure.cell_id.as_deref(), Some("bad"));
    assert_eq!(kernel.status("read").unwrap(), CellStatus::Current);
    assert_eq!(
        output_body(&kernel.notebook().cells[3].outputs[0]),
        "-9223372036854775808"
    );
}

#[test]
fn record_boundary_diagnostics_have_cell_context() {
    for source in [
        "Point(unknown: 1, active: true)",
        "Point(x: 1, x: 2, active: true)",
        "Point(x: 1)",
        "Point(x: true, active: true)",
        "let mut p = Point(x: 1, active: true)",
    ] {
        let mut kernel = NotebookKernel::open(notebook(vec![
            code("def", "record Point:\n    x: i64\n    active: Bool\n"),
            code("bad", source),
        ]))
        .unwrap();
        kernel.run_cell("def").unwrap();
        let error = kernel.run_cell("bad").unwrap_err();
        assert_eq!(error.cell_id.as_deref(), Some("bad"), "{source}");
        assert!(error.message.contains(" at "), "{source}: {error}");
    }
    let error = NotebookKernel::open(notebook(vec![
        code("before", "Point(x: 1, active: true)"),
        code("after", "record Point:\n    x: i64\n    active: Bool\n"),
    ]))
    .err()
    .unwrap();
    assert_eq!(error.code, "CGNK009");
    assert!(error.message.contains("before"));
}

#[test]
fn record_definition_and_value_in_one_cell_are_atomic() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code(
            "both",
            "record Point:\n    x: i64\n    yes: Bool\nlet p = Point(yes: true, x: 7)\np.x",
        ),
        code("later", "p.yes"),
    ]))
    .unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output_body(&kernel.notebook().cells[0].outputs[0]), "7");
    assert_eq!(output_body(&kernel.notebook().cells[1].outputs[0]), "true");
}

#[test]
fn deleting_record_definition_stales_only_dependent_cells_after_reopen() {
    let mut kernel =
        NotebookKernel::open(parse_notebook(include_str!("fixtures/pure-records.cgnb")).unwrap())
            .unwrap();
    kernel.restart_and_run_all().unwrap();
    kernel.remove_cell("point").unwrap();
    assert_eq!(kernel.status("flag").unwrap(), CellStatus::Current);
    assert_eq!(kernel.status("values").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("read").unwrap(), CellStatus::Stale);
    let saved = write_notebook(kernel.notebook()).unwrap();
    let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("flag").unwrap(), CellStatus::Cached);
    assert_eq!(reopened.status("read").unwrap(), CellStatus::Stale);
}

#[test]
fn record_fields_display_in_declaration_order_and_arguments_run_in_source_order() {
    let mut kernel = NotebookKernel::open(notebook(vec![
        code("record", "record Pair:\n    left: i64\n    right: i64\n"),
        code(
            "functions",
            "fn overflow() -> i64:\n    9223372036854775807 + 1\nfn divide() -> i64:\n    1 / 0\n",
        ),
        code("value", "Pair(right: 2, left: 1)"),
        code("first", "Pair(right: divide(), left: overflow())"),
    ]))
    .unwrap();
    for id in ["record", "functions", "value"] {
        kernel.run_cell(id).unwrap();
    }
    assert_eq!(
        output_body(&kernel.notebook().cells[2].outputs[0]),
        "Pair(left: 1, right: 2)"
    );
    let first = kernel.run_cell("first").unwrap_err();
    assert!(first.message.contains("zero"), "{first}");
    kernel
        .replace_cell("first", "Pair(left: overflow(), right: divide())".into())
        .unwrap();
    let second = kernel.rerun_cell("first").unwrap_err();
    assert!(second.message.contains("overflow"), "{second}");
}
