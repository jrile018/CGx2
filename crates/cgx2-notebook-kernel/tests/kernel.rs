use cgx2_notebook_format::{parse_notebook, write_notebook, Cell, CellKind, Notebook, Output};
use cgx2_notebook_kernel::{facts_for_source, NotebookKernel, MAX_CODE_CELLS, MAX_SOURCE_BYTES};
use cgx2_notebook_session::CellStatus;

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
            ordinal: 2,
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
            ordinal: 1,
            body: "10".into()
        }
    );
}

#[test]
fn restart_failure_clears_old_outputs() {
    let mut kernel =
        NotebookKernel::open(notebook(vec![code("a", "let x = 3"), code("b", "x + 4")])).unwrap();
    kernel.restart_and_run_all().unwrap();
    kernel.replace_cell("a", "let x = missing".into()).unwrap();
    assert_eq!(kernel.restart_and_run_all().unwrap_err().code, "CGNK008");
    assert!(kernel
        .notebook()
        .cells
        .iter()
        .all(|cell| cell.outputs.is_empty()));
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
        "fn f() -> i64:\n    1",
        "for x in xs:\n    x",
        "arg_count()",
        "let mut x = 3",
        "x = 4",
    ] {
        let mut kernel = NotebookKernel::open(notebook(vec![code("a", source)])).unwrap();
        assert_eq!(
            kernel.run_cell("a").unwrap_err().code,
            "CGNK001",
            "{source}"
        );
    }
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
