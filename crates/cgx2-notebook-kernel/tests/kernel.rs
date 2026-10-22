use cgx2_notebook_format::{parse_notebook, write_notebook, Cell, CellKind, Notebook, Output};
use cgx2_notebook_kernel::{facts_for_source, NotebookKernel, MAX_CODE_CELLS, MAX_SOURCE_BYTES};
use cgx2_notebook_session::CellStatus;

fn code(id: &str, source: &str) -> Cell {
    Cell {
        id: id.into(),
        kind: CellKind::Code,
        source: source.into(),
        outputs: vec![],
    }
}

fn notebook(cells: Vec<Cell>) -> Notebook {
    Notebook { version: 1, cells }
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
    assert_eq!(facts.reads.into_iter().collect::<Vec<_>>(), vec!["answer"]);
    assert!(!facts.opaque_effects);
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
fn replay_budget_stops_a_long_sequence() {
    let source = format!("\"{}\"", "x".repeat(498));
    let cells = (0..70).map(|n| code(&format!("c{n}"), &source)).collect();
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
