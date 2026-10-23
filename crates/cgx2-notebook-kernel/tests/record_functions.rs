use std::time::Duration;

use cgx2_notebook_format::{parse_notebook, write_notebook, Cell, CellKind, Notebook, Output};
use cgx2_notebook_kernel::{facts_for_source, ExecutionControl, NotebookKernel};
use cgx2_notebook_session::CellStatus;

const EXAMPLE: &str = include_str!("../../cgx2-notebook-cli/examples/record-functions.cgnb");
const RECORD: &str = "record Point:\n    x: i64\n    active: Bool\n";

fn code(id: &str, source: &str) -> Cell {
    Cell {
        id: id.into(),
        kind: CellKind::Code,
        source: source.into(),
        outputs: vec![],
        provenance: None,
    }
}

fn kernel(cells: Vec<Cell>) -> NotebookKernel {
    NotebookKernel::open(Notebook { version: 1, cells }).unwrap()
}

fn output(kernel: &NotebookKernel, id: &str) -> String {
    let cell = kernel
        .notebook()
        .cells
        .iter()
        .find(|cell| cell.id == id)
        .unwrap();
    let Output::Text { body, .. } = &cell.outputs[0] else {
        panic!("expected text")
    };
    body.clone()
}

fn control() -> ExecutionControl {
    ExecutionControl::new(
        env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker").into(),
        Duration::from_secs(5),
    )
}

#[test]
fn record_functions_have_worker_selected_rerun_and_reopen_parity() {
    for worker in [false, true] {
        let mut kernel = NotebookKernel::open(parse_notebook(EXAMPLE).unwrap()).unwrap();
        if worker {
            kernel.set_execution_control(control());
        }
        kernel.restart_and_run_all().unwrap();
        assert_eq!(output(&kernel, "value"), "Point(x: 42, active: true)");
        assert_eq!(output(&kernel, "read"), "42");
        let saved = write_notebook(kernel.notebook()).unwrap();
        let mut reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
        if worker {
            reopened.set_execution_control(control());
        }
        assert_eq!(reopened.status("read").unwrap(), CellStatus::Cached);
        assert_eq!(reopened.rerun_cell("read").unwrap_err().code, "CGNK004");
        reopened.restart_and_run_all().unwrap();
        assert_eq!(output(&reopened, "value"), output(&kernel, "value"));
    }
}

#[test]
fn unused_annotations_are_dependencies_even_when_a_parameter_shadows_the_type() {
    let facts = facts_for_source("fn ignored(Point: Point) -> i64:\n    7\n").unwrap();
    assert!(facts.reads.contains("type:Point"));
    let facts =
        facts_for_source("record Point:\n    x: i64\nfn id(p: Point) -> Point:\n    p\n").unwrap();
    assert!(!facts.reads.contains("type:Point"));
    let mut kernel = kernel(vec![
        code("record", RECORD),
        code("function", "fn ignored(Point: Point) -> i64:\n    7\n"),
        code("read", "ignored(Point(x: 1, active: true))"),
        code("independent", "42"),
    ]);
    kernel.restart_and_run_all().unwrap();
    let independent = kernel.notebook().cells[3].provenance.clone();
    kernel
        .replace_cell(
            "record",
            "record Point:\n    active: Bool\n    x: i64\n".into(),
        )
        .unwrap();
    for id in ["function", "read"] {
        assert_eq!(kernel.status(id).unwrap(), CellStatus::Stale);
    }
    kernel.rerun_affected("record").unwrap();
    assert_eq!(output(&kernel, "read"), "7");
    assert_eq!(kernel.notebook().cells[3].provenance, independent);
    kernel.remove_cell("record").unwrap();
    let saved = write_notebook(kernel.notebook()).unwrap();
    let mut reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("function").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("read").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
    let error = reopened.rerun_cell("function").unwrap_err();
    assert_eq!(error.cell_id.as_deref(), Some("function"));
    assert!(error.message.contains("at "), "{error}");
}

#[test]
fn unicode_same_cell_records_and_exact_scalar_fields_survive_replay() {
    let mut kernel = kernel(vec![
        code("defs", "record 座標:\n    位置: i64\n    有効: Bool\nfn 同一(座標: 座標) -> 座標:\n    座標\nfn 作成(x: i64) -> 座標:\n    座標(有効: true, 位置: x)\n"),
        code("value", "let p = 同一(作成(-9223372036854775807 - 1))\np"),
        code("read", "p.位置"),
    ]);
    kernel.set_execution_control(control());
    kernel.restart_and_run_all().unwrap();
    assert_eq!(
        output(&kernel, "value"),
        "座標(位置: -9223372036854775808, 有効: true)"
    );
    assert_eq!(output(&kernel, "read"), "-9223372036854775808");
}

#[test]
fn unknown_future_qualified_and_generic_annotations_fail_with_cell_spans() {
    for source in [
        "fn f(p: Missing) -> i64:\n    1\n",
        "fn f(p: Point) -> i64:\n    1\n",
        "fn f(p: i64) -> Point:\n    p\n",
        "fn f(p: module.Point) -> i64:\n    1\n",
        "fn f(p: List<Point>) -> i64:\n    1\n",
        "fn f(p: owned<Point>) -> i64:\n    1\n",
    ] {
        let document = Notebook {
            version: 1,
            cells: vec![code("early", source), code("future", RECORD)],
        };
        let error = match NotebookKernel::open(document) {
            Err(error) => error,
            Ok(mut kernel) => {
                kernel.rerun_cell("future").unwrap();
                kernel.rerun_cell("early").unwrap_err()
            }
        };
        assert_eq!(error.cell_id.as_deref(), Some("early"));
        assert!(error.message.contains("at "), "{source}: {error}");
    }
}

#[test]
fn nominal_mismatch_and_failed_cells_never_publish_pending_values_or_definitions() {
    let mut kernel = kernel(vec![
        code(
            "defs",
            "record A:\n    x: i64\nrecord B:\n    x: i64\nfn id(p: A) -> A:\n    p\n",
        ),
        code("good", "let a = id(A(x: 9))"),
        code("bad", "id(B(x: 9))"),
        code("reader", "a.x"),
    ]);
    kernel.run_cell("defs").unwrap();
    kernel.run_cell("good").unwrap();
    assert!(kernel.rerun_cell("bad").is_err());
    kernel.rerun_cell("reader").unwrap();
    assert_eq!(output(&kernel, "reader"), "9");
    kernel.replace_cell("bad", "record New:\n    x: i64\nfn fresh(p: New) -> New:\n    p\nlet pending = fresh(New(x: 4))\n1 / 0\n".into()).unwrap();
    assert!(kernel.rerun_cell("bad").is_err());
    kernel
        .replace_cell("reader", "fresh(New(x: 2))".into())
        .unwrap();
    assert!(kernel.rerun_cell("reader").is_err());
    kernel.replace_cell("reader", "a.x".into()).unwrap();
    kernel.rerun_cell("reader").unwrap();
    assert_eq!(output(&kernel, "reader"), "9");
    kernel.replace_cell("bad", "id(A(x: 10))".into()).unwrap();
    kernel.rerun_cell("bad").unwrap();
    assert_eq!(output(&kernel, "bad"), "A(x: 10)");
}

#[test]
fn value_shadowing_cannot_hide_a_record_annotation_dependency() {
    let mut kernel = kernel(vec![
        code("record", RECORD),
        code("shadow", "let Point = 12"),
        code("function", "fn ignored(Point: Point) -> i64:\n    7\n"),
        code("independent", "Point + 1"),
    ]);
    kernel.restart_and_run_all().unwrap();
    kernel.remove_cell("record").unwrap();
    assert_eq!(kernel.status("function").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
    let saved = write_notebook(kernel.notebook()).unwrap();
    let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
    assert_eq!(reopened.status("function").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
}

#[test]
fn record_functions_keep_source_argument_order_and_checked_arithmetic() {
    for worker in [false, true] {
        let mut kernel = kernel(vec![
            code("defs", "record Pair:\n    left: i64\n    right: i64\nfn pair(left: i64, right: i64) -> Pair:\n    Pair(right: right, left: left)\nfn overflow() -> i64:\n    9223372036854775807 + 1\nfn divide() -> i64:\n    1 / 0\n"),
            code("call", "pair(divide(), overflow())"),
        ]);
        if worker {
            kernel.set_execution_control(control());
        }
        kernel.run_cell("defs").unwrap();
        let error = kernel.run_cell("call").unwrap_err();
        assert!(error.message.contains("division by zero"), "{error}");
        kernel
            .replace_cell("call", "pair(overflow(), divide())".into())
            .unwrap();
        let error = kernel.rerun_cell("call").unwrap_err();
        assert!(error.message.contains("overflow"), "{error}");
        kernel.replace_cell("call", "pair(1, 2)".into()).unwrap();
        kernel.rerun_cell("call").unwrap();
        assert_eq!(output(&kernel, "call"), "Pair(left: 1, right: 2)");
    }
}

#[test]
fn unsupported_record_shapes_effects_contracts_and_captures_stay_rejected() {
    for source in [
        "record Bad:\n    p: Point\nfn id(p: Bad) -> Bad:\n    p\n",
        "record Bad:\n    text: String\nfn id(p: Bad) -> Bad:\n    p\n",
        "fn id(p: Point) -> Point effects(network):\n    p\n",
        "fn id(p: Point) -> Point caps(net):\n    p\n",
        "fn id(p: Point) -> Point requires true:\n    p\n",
        "fn id(p: Point) -> Point:\n    let mut q = p\n    q\n",
        "fn id(p: Point) -> Point:\n    captured\n",
        "fn id(p: Point) -> Point:\n    id(p)\n",
    ] {
        let mut kernel = kernel(vec![code("record", RECORD), code("bad", source)]);
        kernel.run_cell("record").unwrap();
        let error = kernel.run_cell("bad").unwrap_err();
        assert_eq!(error.cell_id.as_deref(), Some("bad"), "{source}: {error}");
        assert_eq!(kernel.status("record").unwrap(), CellStatus::Current);
    }
}

#[test]
fn cancellation_and_static_function_and_step_limits_still_apply() {
    let mut kernel = kernel(vec![
        code("record", RECORD),
        code("function", "fn id(p: Point) -> Point:\n    p\n"),
        code("call", "id(Point(x: 1, active: true))"),
    ]);
    kernel.run_cell("record").unwrap();
    let control = control();
    kernel.set_execution_control(control.clone());
    control.interrupt();
    assert_eq!(kernel.run_cell("function").unwrap_err().code, "CGNK010");
    assert_eq!(kernel.status("record").unwrap(), CellStatus::Current);
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output(&kernel, "call"), "Point(x: 1, active: true)");

    // Each definition is small, but repeated calls exceed the static step budget.
    let mut source = String::from("record P:\n    x: i64\nfn f0(p: P) -> P:\n    p\n");
    for index in 1..18 {
        source.push_str(&format!(
            "fn f{index}(p: P) -> P:\n    let a = f{}(p)\n    f{}(a)\n",
            index - 1,
            index - 1
        ));
    }
    kernel.replace_cell("function", source).unwrap();
    assert_eq!(kernel.rerun_cell("function").unwrap_err().code, "CGNK006");

    // A 32-function chain is admitted; another definition exceeds the function bound.
    let mut source = String::from("fn f0(p: Point) -> Point:\n    p\n");
    for index in 1..32 {
        source.push_str(&format!(
            "fn f{index}(p: Point) -> Point:\n    f{}(p)\n",
            index - 1
        ));
    }
    kernel.replace_cell("function", source).unwrap();
    kernel.rerun_cell("function").unwrap();
    kernel
        .replace_cell("call", "fn extra(p: Point) -> Point:\n    f31(p)\n".into())
        .unwrap();
    assert_eq!(kernel.rerun_cell("call").unwrap_err().code, "CGNK006");
}

#[test]
fn cumulative_replay_budget_and_zero_deadline_fail_without_publishing() {
    let mut kernel = kernel(vec![
        code("record", RECORD),
        code(
            "function",
            &format!(
                "fn id(p: Point) -> Point:\n    let padding = \"{}\"\n    p\n",
                "a".repeat(24_000)
            ),
        ),
        code(
            "call",
            &(0..48)
                .map(|index| format!("let p{index} = id(Point(x: 1, active: true))\n"))
                .collect::<String>(),
        ),
    ]);
    kernel.run_cell("record").unwrap();
    kernel.run_cell("function").unwrap();
    let error = kernel.run_cell("call").unwrap_err();
    assert_eq!(error.code, "CGNK006", "{error}");
    assert_eq!(kernel.status("function").unwrap(), CellStatus::Current);
    kernel.replace_cell("call", "p0.x".into()).unwrap();
    assert!(kernel.rerun_cell("call").is_err());
    kernel
        .replace_cell("call", "id(Point(x: 2, active: false))".into())
        .unwrap();
    // Replay bytes are cumulative across the session; restart renews the budget.
    kernel.restart().unwrap();
    kernel.run_cell("record").unwrap();
    kernel.run_cell("function").unwrap();
    kernel.set_execution_control(ExecutionControl::new(
        env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker").into(),
        Duration::ZERO,
    ));
    assert_eq!(kernel.rerun_cell("call").unwrap_err().code, "CGNK010");
    kernel.set_execution_control(control());
    kernel.rerun_cell("call").unwrap();
    assert_eq!(output(&kernel, "call"), "Point(x: 2, active: false)");
}

#[test]
fn selected_record_function_execution_never_replays_an_effect_cell() {
    let mut kernel = kernel(vec![
        code("effect", "foreign_call()"),
        code("record", RECORD),
        code("function", "fn id(p: Point) -> Point:\n    p\n"),
        code("call", "id(Point(x: 1, active: true))"),
    ]);
    kernel.set_execution_control(control());
    for id in ["record", "function", "call"] {
        kernel.rerun_cell(id).unwrap();
    }
    assert_eq!(kernel.status("effect").unwrap(), CellStatus::NeverRun);
    assert!(kernel.notebook().cells[0].outputs.is_empty());
    assert!(kernel.notebook().cells[0].provenance.is_none());
    assert_eq!(output(&kernel, "call"), "Point(x: 1, active: true)");
}

#[test]
fn conditional_inactive_dependencies_worker_reopen_and_failure_recovery() {
    let example = include_str!("../../cgx2-notebook-cli/examples/conditionals.cgnb");
    for worker in [false, true] {
        let mut kernel = NotebookKernel::open(parse_notebook(example).unwrap()).unwrap();
        if worker {
            kernel.set_execution_control(control());
        }
        kernel.restart_and_run_all().unwrap();
        assert_eq!(output(&kernel, "value"), "42");
        kernel
            .replace_cell("fallback", "fn fallback() -> i64:\n    9".into())
            .unwrap();
        assert_eq!(kernel.status("functions").unwrap(), CellStatus::Stale);
        assert_eq!(kernel.status("value").unwrap(), CellStatus::Stale);
        assert!(matches!(
            kernel.status("independent").unwrap(),
            CellStatus::Current | CellStatus::Cached
        ));
        kernel.rerun_affected("fallback").unwrap();
        assert_eq!(output(&kernel, "value"), "42");
        let saved = write_notebook(kernel.notebook()).unwrap();
        let mut reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
        if worker {
            reopened.set_execution_control(control());
        }
        reopened.restart_and_run_all().unwrap();
        reopened
            .replace_cell(
                "value",
                "let pending = choose(false)\nchoose(true) / 0".into(),
            )
            .unwrap();
        assert!(reopened.rerun_cell("value").is_err());
        reopened.replace_cell("value", "pending".into()).unwrap();
        assert!(reopened.rerun_cell("value").is_err());
        reopened
            .replace_cell("value", "choose(false)".into())
            .unwrap();
        reopened.rerun_cell("value").unwrap();
        assert_eq!(output(&reopened, "value"), "9");
        reopened.restart_and_run_all().unwrap();
        assert_eq!(output(&reopened, "value"), "9");
    }
}

#[test]
fn conditional_dependencies_keep_sibling_and_nested_scopes_separate() {
    let facts = facts_for_source("fn choose(flag: Bool) -> i64:\n    if flag:\n        let outside = 1\n        return outside\n    else:\n        if false:\n            let outside = 2\n            return outside\n        else:\n            return outside()\n").unwrap();
    assert!(facts.reads.contains("outside"));
    assert!(!facts.reads.contains("flag"));
    assert!(facts_for_source("if true:\n    1\nelse:\n    2\n").is_err());
    assert!(facts_for_source(
        "fn bad() -> i64:\n    if true:\n        return 1\n    else:\n        return arg_count()\n"
    )
    .is_err());
}
