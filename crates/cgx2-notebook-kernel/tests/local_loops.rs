use std::path::PathBuf;
use std::time::Duration;

use cgx2_notebook_format::{parse_notebook, write_notebook, Cell, CellKind, Notebook, Output};
use cgx2_notebook_kernel::{facts_for_source, ExecutionControl, NotebookKernel};
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

fn kernel(cells: Vec<Cell>, worker: bool) -> NotebookKernel {
    let mut kernel = NotebookKernel::open(Notebook { version: 1, cells }).unwrap();
    if worker {
        kernel.set_execution_control(control(Duration::from_secs(2)));
    }
    kernel
}

fn control(timeout: Duration) -> ExecutionControl {
    ExecutionControl::new(
        PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
        timeout,
    )
}

fn output(kernel: &NotebookKernel, id: &str) -> String {
    let cell = kernel
        .notebook()
        .cells
        .iter()
        .find(|cell| cell.id == id)
        .unwrap();
    match &cell.outputs[0] {
        Output::Text { body, .. } => body.clone(),
        _ => panic!("expected text"),
    }
}

const SUM: &str = "fn sum(x: i64) -> i64 effects(Mutate):\n    let mut total: i64 = x\n    for i in range(-2, 3):\n        if i < 0:\n            total = total - i\n        else:\n            for j in range(0, 2):\n                total += i + j\n    total\n";

#[test]
fn nested_local_computation_runs_direct_and_worker_and_reinitializes() {
    for worker in [false, true] {
        let mut k = kernel(
            vec![
                code("defs", SUM),
                code("input", "let x = 2"),
                code("value", "sum(x)"),
                code("other", "42"),
            ],
            worker,
        );
        k.restart_and_run_all().unwrap();
        assert_eq!(output(&k, "value"), "14");
        let other = k.notebook().cells[3].clone();
        k.rerun_cell("value").unwrap();
        assert_eq!(output(&k, "value"), "14");
        k.replace_cell("input", "let x = 3".into()).unwrap();
        k.rerun_affected("input").unwrap();
        assert_eq!(output(&k, "value"), "15");
        assert_eq!(k.notebook().cells[3], other);
        let saved = write_notebook(k.notebook()).unwrap();
        let mut reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
        assert_eq!(reopened.status("value").unwrap(), CellStatus::Cached);
        reopened.rerun_cell("defs").unwrap();
        reopened.rerun_affected("input").unwrap();
        assert_eq!(output(&reopened, "value"), "15");
        reopened.restart_and_run_all().unwrap();
        assert_eq!(output(&reopened, "value"), "15");
    }
}

#[test]
fn literal_ranges_cover_empty_negative_and_integer_boundaries() {
    for worker in [false, true] {
        for (start, end, expected) in [
            ("0", "0", "0"),
            ("0", "1", "1"),
            ("0", "3", "3"),
            ("3", "-2", "0"),
            ("-3", "-1", "2"),
            ("-9223372036854775808", "-9223372036854775807", "1"),
            ("9223372036854775806", "9223372036854775807", "1"),
            ("9223372036854775807", "-9223372036854775808", "0"),
        ] {
            let source = format!("fn f() -> i64:\n    let mut n = 0\n    for i in range({start}, {end}):\n        n += 1\n    n\n");
            let mut k = kernel(vec![code("defs", &source), code("call", "f()")], worker);
            k.restart_and_run_all().unwrap();
            assert_eq!(output(&k, "call"), expected, "{start}..{end}");
        }
    }
}

#[test]
fn lexical_shadowing_preserves_nearest_mutability_and_scope() {
    let source = "fn f(x: i64) -> i64:\n    let mut n = x\n    if true:\n        let mut x = n\n        x += 8\n        n = x\n    else:\n        0\n    for i in range(0, 2):\n        let mut item = i\n        item += 1\n        n += item\n    n\n";
    let mut k = kernel(vec![code("defs", source), code("call", "f(2)")], true);
    k.restart_and_run_all().unwrap();
    assert_eq!(output(&k, "call"), "13");
    for source in [
        "fn f() -> i64:\n    let mut n = 0\n    if true:\n        let n = 1\n        n += 1\n    else:\n        0\n    n\n",
        "fn f() -> i64:\n    if true:\n        let mut n = 1\n    else:\n        0\n    n += 1\n    0\n",
        "fn f() -> i64:\n    for i in range(0, 1):\n        let mut n = 1\n    n\n",
        "fn f() -> i64:\n    let mut i = 1\n    for i in range(0, 1):\n        i += 1\n    i\n",
    ] {
        assert!(facts_for_source(source).is_err(), "{source}");
    }
}

#[test]
fn forbidden_mutation_types_effects_iterators_and_operations_fail_closed() {
    let cases = [
        "fn f(x: i64) -> i64:\n    x += 1\n    x\n",
        "fn f() -> i64:\n    captured = 2\n    0\n",
        "fn f() -> i64:\n    let mut b = true\n    b = false\n    0\n",
        "fn f() -> i64:\n    let mut s = \"x\"\n    0\n",
        "record P:\n    x: i64\nfn f(p: P) -> i64:\n    p.x = 2\n    p.x\n",
        "record P:\n    x: i64\nfn f() -> i64:\n    let mut p = P(x: 1)\n    0\n",
        "fn f() -> i64:\n    let mut n = 0\n    n = true\n    n\n",
        "fn f() -> i64:\n    let mut n: Bool = false\n    0\n",
        "fn f() -> i64 effects():\n    let mut n = 0\n    n += 1\n    n\n",
        "fn f() -> i64 effects(Mutate, IO):\n    0\n",
        "fn f() -> i64 caps(ReadFile):\n    0\n",
        "fn f() -> i64 requires true:\n    0\n",
        "fn f() -> i64:\n    no_gc(budget: 1):\n        0\n    0\n",
        "fn f() -> i64:\n    for i in range(0, 0):\n        return 1\n    0\n",
        "fn f() -> i64:\n    for i in range(0, 0):\n        memory.collect(1)\n    0\n",
        "fn f() -> i64:\n    for i in range(0, 0):\n        foreign_call()\n    0\n",
        "for i in range(0, 1):\n    i\n",
        "let mut n = 0\nn += 1\n",
    ];
    for source in cases {
        let mut k = kernel(
            vec![
                code("captured", "let captured = 3"),
                code("defs", source),
                code("other", "42"),
            ],
            false,
        );
        k.run_cell("captured").unwrap();
        assert!(k.run_cell("defs").is_err(), "admitted {source}");
        k.rerun_cell("other").unwrap();
        assert_eq!(output(&k, "other"), "42");
    }
    for iter in [
        "range(0, x)",
        "range(0, 1 + 1)",
        "range(0, f())",
        "range(0)",
        "range(0, 1, 1)",
        "range(start: 0, end: 1)",
        "range(0, 9223372036854775808)",
        "range(-9223372036854775809, 0)",
        "x",
    ] {
        let source = format!("fn f(x: i64) -> i64:\n    for i in {iter}:\n        0\n    0\n");
        assert!(facts_for_source(&source).is_err(), "{iter}");
    }
}

#[test]
fn mutation_propagates_through_calls_but_cannot_enter_a_range_body() {
    let helper = "fn local() -> i64:\n    let mut n = 1\n    n += 1\n    n\nfn wrapper() -> i64:\n    local()\n";
    let mut k = kernel(vec![code("defs", helper), code("call", "wrapper()")], true);
    k.restart_and_run_all().unwrap();
    assert_eq!(output(&k, "call"), "2");
    for (annotation, call) in [
        ("", "local()"),
        ("", "wrapper()"),
        (" effects()", "wrapper()"),
    ] {
        let source = format!("{helper}fn f() -> i64{annotation}:\n    let mut n = 0\n    for i in range(0, 0):\n        n += {call}\n    n\n");
        let mut k = kernel(vec![code("defs", &source)], false);
        let error = k.run_cell("defs").unwrap_err();
        assert!(error.message.contains("CGT012"), "{error}");
    }
    let mut k = kernel(
        vec![code(
            "defs",
            &format!("{helper}fn dishonest() -> i64 effects():\n    wrapper()\n"),
        )],
        false,
    );
    assert!(k.run_cell("defs").unwrap_err().message.contains("CGT012"));
}

#[test]
fn empty_loops_inactive_branches_and_assignment_rhs_remain_dependencies() {
    let source = "fn sum() -> i64:\n    let mut n = 0\n    for i in range(0, 0):\n        if false:\n            n = helper()\n        else:\n            0\n    n\n";
    let facts = facts_for_source(source).unwrap();
    assert_eq!(facts.reads.into_iter().collect::<Vec<_>>(), ["helper"]);
    let mut k = kernel(
        vec![
            code("helper", "fn helper() -> i64 effects():\n    1\n"),
            code("defs", source),
            code("call", "sum()"),
            code("other", "42"),
        ],
        false,
    );
    k.restart_and_run_all().unwrap();
    k.replace_cell("helper", "fn helper() -> i64 effects():\n    2\n".into())
        .unwrap();
    assert_eq!(k.status("defs").unwrap(), CellStatus::Stale);
    assert_eq!(k.status("call").unwrap(), CellStatus::Stale);
    assert_eq!(k.status("other").unwrap(), CellStatus::Current);
    k.rerun_affected("helper").unwrap();
    assert_eq!(output(&k, "call"), "0");
    k.remove_cell("helper").unwrap();
    assert!(k.rerun_cell("defs").is_err());
    assert_eq!(k.status("other").unwrap(), CellStatus::Current);
}

#[test]
fn overflow_failure_is_transactional_and_locals_recover_cleanly() {
    for worker in [false, true] {
        let source = "fn f(x: i64) -> i64:\n    let mut n = x\n    for i in range(0, 2):\n        n += 1\n    n\n";
        let mut k = kernel(
            vec![
                code("defs", source),
                code("base", "let base = 40"),
                code(
                    "call",
                    "let partial = f(1)\nlet broken = f(9223372036854775807)",
                ),
                code("probe", "partial"),
                code("other", "42"),
            ],
            worker,
        );
        k.run_cell("defs").unwrap();
        k.run_cell("base").unwrap();
        let error = k.run_cell("call").unwrap_err();
        assert!(error.message.contains("CGR005"), "{error}");
        assert!(k.rerun_cell("probe").is_err());
        assert_eq!(k.status("base").unwrap(), CellStatus::Current);
        k.replace_cell("call", "let partial = f(base)".into())
            .unwrap();
        k.rerun_affected("call").unwrap();
        k.rerun_cell("probe").unwrap();
        assert_eq!(output(&k, "probe"), "42");
        k.rerun_cell("other").unwrap();
    }
}

#[test]
fn static_loop_call_and_replay_bounds_reject_before_worker_or_definitions() {
    for body in [
        "    for i in range(-9223372036854775808, 9223372036854775807):\n        0\n    0\n",
        "    for i in range(0, 1000):\n        for j in range(0, 1000):\n            0\n    0\n",
    ] {
        let mut k = kernel(
            vec![
                code("defs", &format!("fn f() -> i64:\n{body}")),
                code("call", "f()"),
            ],
            false,
        );
        k.set_execution_control(ExecutionControl::new(
            PathBuf::from("/nonexistent-wave28-worker"),
            Duration::from_secs(2),
        ));
        assert_eq!(k.run_cell("defs").unwrap_err().code, "CGNK006");
        assert!(k.rerun_cell("call").is_err());
    }
    // Each function costs 50,000 steps; the two calls in one cell cannot run.
    let defs =
        "fn f() -> i64:\n    let mut n = 0\n    for i in range(0, 16664):\n        n += 1\n    n\n";
    let mut k = kernel(vec![code("defs", defs), code("call", "f()\nf()")], false);
    k.run_cell("defs").unwrap();
    k.set_execution_control(ExecutionControl::new(
        PathBuf::from("/nonexistent-wave28-worker"),
        Duration::from_secs(2),
    ));
    assert_eq!(k.run_cell("call").unwrap_err().code, "CGNK006");
}

#[test]
fn interrupted_deadline_and_bad_worker_leave_no_partial_bindings() {
    for mode in 0..3 {
        let mut k = kernel(
            vec![
                code("defs", SUM),
                code("call", "let answer = sum(2)"),
                code("read", "answer"),
            ],
            false,
        );
        k.run_cell("defs").unwrap();
        let c = if mode == 2 {
            ExecutionControl::new(
                PathBuf::from("/nonexistent-wave28-worker"),
                Duration::from_secs(2),
            )
        } else {
            control(if mode == 1 {
                Duration::ZERO
            } else {
                Duration::from_secs(2)
            })
        };
        if mode == 0 {
            c.interrupt();
        }
        k.set_execution_control(c);
        assert_eq!(k.run_cell("call").unwrap_err().code, "CGNK010");
        assert!(k.rerun_cell("read").is_err());
        k.set_execution_control(control(Duration::from_secs(2)));
        k.restart_and_run_all().unwrap();
        assert_eq!(output(&k, "read"), "14");
    }
}

#[test]
fn exact_static_boundary_executes_in_both_paths() {
    for worker in [false, true] {
        let defs = "fn f() -> i64:\n    let mut n = 0\n    for i in range(0, 33330):\n        n += 1\n    n\n";
        let mut k = kernel(vec![code("defs", defs), code("call", "f()")], worker);
        k.restart_and_run_all().unwrap();
        assert_eq!(output(&k, "call"), "33330");
        k.replace_cell("call", "let answer = f()".into()).unwrap();
        assert_eq!(k.rerun_cell("call").unwrap_err().code, "CGNK006");
    }
}

const EXAMPLE: &str = include_str!("../../cgx2-notebook-cli/examples/local-loops/local-loops.cgnb");

#[test]
fn imported_pure_helpers_inside_loops_keep_dependency_and_nominal_rules() {
    let project =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../cgx2-notebook-cli/examples/local-loops");
    for worker in [false, true] {
        let mut k =
            NotebookKernel::open_with_project(parse_notebook(EXAMPLE).unwrap(), Some(&project))
                .unwrap();
        if worker {
            k.set_execution_control(control(Duration::from_secs(2)));
        }
        k.restart_and_run_all().unwrap();
        assert_eq!(output(&k, "value"), "20");
        let independent = k.notebook().cells.last().unwrap().clone();
        k.replace_cell(
            "functions",
            k.notebook().cells[3]
                .source
                .replace("math.bump(i + j)", "math.bump(i + j) + 1"),
        )
        .unwrap();
        k.rerun_affected("functions").unwrap();
        assert_eq!(output(&k, "value"), "26");
        assert_eq!(k.notebook().cells.last().unwrap(), &independent);
        let saved = write_notebook(k.notebook()).unwrap();
        let mut reopened =
            NotebookKernel::open_with_project(parse_notebook(&saved).unwrap(), Some(&project))
                .unwrap();
        reopened.restart_and_run_all().unwrap();
        assert_eq!(output(&reopened, "value"), "26");
        // Empty loops still lower and resolve imported calls and type annotations.
        for body in [
            "        total = math.missing(i)",
            "        let p: math.Missing = i",
        ] {
            let source = format!("fn f() -> i64:\n    let mut total = 0\n    for i in range(0, 0):\n{body}\n    total\n");
            assert!(k.replace_cell("functions", source).is_err());
        }
    }
}

#[test]
fn imported_functions_keep_the_wave27_profile() {
    let root =
        std::env::temp_dir().join(format!("cgx2-loop-import-profile-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("cgx2.toml"),
        include_str!("../../cgx2-notebook-cli/examples/local-loops/cgx2.toml"),
    )
    .unwrap();
    for body in [
        "    let mut n = x\n    n += 1\n    n\n",
        "    for i in range(0, 1):\n        x\n    x\n",
    ] {
        std::fs::write(
            root.join("src/math.cg"),
            format!("module compute.math\npub fn bump(x: i64) -> i64:\n{body}"),
        )
        .unwrap();
        assert!(
            NotebookKernel::open_with_project(parse_notebook(EXAMPLE).unwrap(), Some(&root))
                .is_err()
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn checked_set_arithmetic_overflow_does_not_publish_a_definition_or_value() {
    for worker in [false, true] {
        for assignment in ["n = n + 1", "n = n * 2", "n = n / 0"] {
            let defs = format!("fn f() -> i64:\n    let mut n = 9223372036854775807\n    for i in range(0, 1):\n        {assignment}\n    n\n");
            let mut k = kernel(
                vec![
                    code("defs", &defs),
                    code("call", "let answer = f()"),
                    code("read", "answer"),
                ],
                worker,
            );
            k.run_cell("defs").unwrap();
            assert!(k.run_cell("call").is_err());
            assert!(k.rerun_cell("read").is_err());
        }
    }
}

#[test]
fn annotations_inside_empty_loops_remain_schema_dependencies() {
    let defs =
        "fn f(p: Point) -> i64:\n    for i in range(0, 0):\n        let q: Point = p\n    0\n";
    let facts = facts_for_source(defs).unwrap();
    assert!(facts.reads.contains("type:Point"));
    let mut k = kernel(
        vec![
            code("record", "record Point:\n    x: i64\n"),
            code("defs", defs),
            code("call", "f(Point(x: 1))"),
            code("other", "42"),
        ],
        false,
    );
    k.restart_and_run_all().unwrap();
    k.replace_cell("record", "record Point:\n    x: Bool\n".into())
        .unwrap();
    assert_eq!(k.status("defs").unwrap(), CellStatus::Stale);
    assert_eq!(k.status("call").unwrap(), CellStatus::Stale);
    assert_eq!(k.status("other").unwrap(), CellStatus::Current);
    assert!(k.rerun_affected("record").is_err());
}

#[cfg(unix)]
#[test]
fn interrupt_during_loop_worker_keeps_prior_cells_and_recovers() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;
    let root = std::env::temp_dir().join(format!("cgx2-loop-interrupt-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let worker = root.join("worker");
    let marker = root.join("started");
    std::fs::write(
        &worker,
        format!("#!/bin/sh\ntouch {}\nexec sleep 2\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut k = kernel(
        vec![
            code("defs", SUM),
            code("prior", "42"),
            code("call", "let answer = sum(2)"),
            code("read", "answer"),
        ],
        false,
    );
    k.run_cell("defs").unwrap();
    k.run_cell("prior").unwrap();
    let prior = k.notebook().cells[1].clone();
    let c = ExecutionControl::new(worker, Duration::from_secs(2));
    k.set_execution_control(c.clone());
    let running = std::thread::spawn(move || {
        let result = k.run_cell("call");
        (k, result)
    });
    let start = Instant::now();
    while !marker.exists() && start.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(marker.exists(), "worker was not started");
    c.interrupt();
    let (mut k, result) = running.join().unwrap();
    assert_eq!(result.unwrap_err().code, "CGNK010");
    assert_eq!(k.notebook().cells[1], prior);
    assert!(k.rerun_cell("read").is_err());
    k.set_execution_control(control(Duration::from_secs(2)));
    k.rerun_cell("call").unwrap();
    k.rerun_cell("read").unwrap();
    assert_eq!(output(&k, "read"), "14");
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn malformed_and_wrong_type_loop_worker_frames_never_publish() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("cgx2-loop-frames-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let worker = root.join("worker");
    for frame in ["Btrue", "Igarbage", "Rbroken"] {
        std::fs::write(&worker, format!("#!/bin/sh\nprintf '{frame}'\n")).unwrap();
        std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut k = kernel(
            vec![
                code("defs", SUM),
                code("call", "let answer = sum(2)"),
                code("read", "answer"),
            ],
            false,
        );
        k.run_cell("defs").unwrap();
        k.set_execution_control(ExecutionControl::new(
            worker.clone(),
            Duration::from_secs(2),
        ));
        assert_eq!(k.run_cell("call").unwrap_err().code, "CGNK010");
        assert!(k.rerun_cell("read").is_err());
        k.set_execution_control(control(Duration::from_secs(2)));
        k.restart_and_run_all().unwrap();
        assert_eq!(output(&k, "read"), "14");
    }
    std::fs::remove_dir_all(root).unwrap();
}
