use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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
        kernel.set_execution_control(ExecutionControl::new(
            PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
            Duration::from_secs(2),
        ));
    }
    kernel
}

fn output(kernel: &NotebookKernel, id: &str) -> String {
    let cell = kernel.notebook().cells.iter().find(|c| c.id == id).unwrap();
    match &cell.outputs[0] {
        Output::Text { body, .. } => body.clone(),
        _ => panic!("expected text"),
    }
}

const SUM: &str = "fn sum(x: i64) -> i64 effects(Mutate):\n    requires x >= 0\n    ensures result == x + 6\n    let mut n = x\n    for i in range(0, 4):\n        n += i\n    n\n";

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-contracts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        for (name, source) in [
            (
                "cgx2.toml",
                include_str!("../../cgx2-notebook-cli/examples/contract-imports/cgx2.toml"),
            ),
            (
                "src/base.cg",
                include_str!("../../cgx2-notebook-cli/examples/contract-imports/src/base.cg"),
            ),
            (
                "src/guards.cg",
                include_str!("../../cgx2-notebook-cli/examples/contract-imports/src/guards.cg"),
            ),
        ] {
            std::fs::write(root.join(name), source).unwrap();
        }
        Self(root)
    }

    fn open(&self, book: Notebook, worker: bool) -> NotebookKernel {
        let mut k = NotebookKernel::open_with_project(book, Some(&self.0)).unwrap();
        if worker {
            k.set_execution_control(ExecutionControl::new(
                PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
                Duration::from_secs(2),
            ));
        }
        k
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn contracted_accumulators_run_rerun_restart_and_reopen_in_both_modes() {
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
        assert_eq!(output(&k, "value"), "8");
        let independent = k.notebook().cells[3].clone();
        k.rerun_cell("value").unwrap();
        k.replace_cell("input", "let x = 5".into()).unwrap();
        k.rerun_affected("input").unwrap();
        assert_eq!(output(&k, "value"), "11");
        assert_eq!(k.notebook().cells[3], independent);
        let saved = write_notebook(k.notebook()).unwrap();
        let mut reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
        assert_eq!(reopened.status("value").unwrap(), CellStatus::Cached);
        reopened.restart_and_run_all().unwrap();
        assert_eq!(output(&reopened, "value"), "11");
    }
}

#[test]
fn parameter_result_and_multiple_conditions_use_checked_scalar_semantics() {
    let defs = "fn id(x: i64, ok: Bool) -> i64:\n    requires ok\n    requires x >= -9223372036854775808\n    ensures result == x\n    ensures result <= 9223372036854775807\n    return x\nfn yes(x: Bool) -> Bool:\n    ensures result == x\n    x\n";
    for worker in [false, true] {
        let mut k = kernel(
            vec![
                code("defs", defs),
                code("min", "id(-9223372036854775808, true)"),
                code("max", "id(9223372036854775807, true)"),
                code("bool", "yes(false)"),
            ],
            worker,
        );
        k.restart_and_run_all().unwrap();
        assert_eq!(output(&k, "min"), i64::MIN.to_string());
        assert_eq!(output(&k, "max"), i64::MAX.to_string());
        assert_eq!(output(&k, "bool"), "false");
    }
}

#[test]
fn failed_false_or_overflowing_contracts_publish_no_partial_values() {
    for worker in [false, true] {
        for (condition, expected) in [
            ("requires false", "CGR015"),
            ("ensures false", "CGR016"),
            ("requires 9223372036854775807 + 1 > 0", "CGR"),
            ("ensures -9223372036854775808 - 1 < 0", "CGR"),
            ("ensures 1 / 0 == 0", "CGR"),
        ] {
            let defs = format!("fn f() -> i64:\n    {condition}\n    7\n");
            let mut k = kernel(
                vec![
                    code("stable", "let stable = 42"),
                    code("defs", &defs),
                    code("bad", "let pending = 3\nlet answer = f()\n"),
                    code("pending", "pending"),
                    code("answer", "answer"),
                    code("other", "stable"),
                ],
                worker,
            );
            k.run_cell("stable").unwrap();
            k.run_cell("defs").unwrap();
            let failure = k.run_cell("bad").unwrap_err();
            assert!(failure.message.contains(expected), "{condition}: {failure}");
            assert_eq!(failure.cell_id.as_deref(), Some("bad"));
            assert_eq!(k.status("bad").unwrap(), CellStatus::Failed);
            for id in ["pending", "answer"] {
                assert!(k.rerun_cell(id).is_err(), "{condition}: leaked {id}");
            }
            k.rerun_cell("other").unwrap();
            assert_eq!(output(&k, "other"), "42");
            let saved = write_notebook(k.notebook()).unwrap();
            let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
            assert_eq!(reopened.status("bad").unwrap(), CellStatus::Failed);
        }
    }
}

#[test]
fn contracts_precede_body_and_postconditions_check_explicit_return() {
    for worker in [false, true] {
        for (source, expected) in [
            ("fn f() -> i64:\n    requires false\n    1 / 0\n", "CGR015"),
            (
                "fn f() -> i64:\n    ensures result == 2\n    return 1\n",
                "CGR016",
            ),
        ] {
            let mut k = kernel(vec![code("defs", source), code("call", "f()")], worker);
            k.run_cell("defs").unwrap();
            assert!(k.run_cell("call").unwrap_err().message.contains(expected));
        }
    }
}

#[test]
fn contract_only_dependencies_drive_edits_deletion_and_saved_staleness() {
    let defs = "fn f(x: i64) -> i64:\n    requires positive(x)\n    ensures valid(result)\n    x\n";
    let facts = facts_for_source(defs).unwrap();
    assert_eq!(
        facts.reads,
        ["positive".to_owned(), "valid".to_owned()].into()
    );
    for worker in [false, true] {
        let mut k = kernel(
            vec![
                code("pre", "fn positive(x: i64) -> Bool:\n    x > 0\n"),
                code("post", "fn valid(x: i64) -> Bool:\n    x > 1\n"),
                code("defs", defs),
                code("value", "f(2)"),
                code("other", "42"),
            ],
            worker,
        );
        k.restart_and_run_all().unwrap();
        let independent = k.notebook().cells[4].clone();
        k.replace_cell("post", "fn valid(x: i64) -> Bool:\n    x > 2\n".into())
            .unwrap();
        assert_eq!(k.status("defs").unwrap(), CellStatus::Stale);
        assert_eq!(k.status("value").unwrap(), CellStatus::Stale);
        assert_eq!(k.notebook().cells[4], independent);
        let saved = write_notebook(k.notebook()).unwrap();
        let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
        assert_eq!(reopened.status("value").unwrap(), CellStatus::Stale);
        assert_eq!(reopened.status("other").unwrap(), CellStatus::Cached);
        assert!(k
            .rerun_affected("post")
            .unwrap_err()
            .message
            .contains("CGR016"));
        assert_eq!(output(&k, "value"), "2");
        k.remove_cell("pre").unwrap();
        let failure = k.rerun_cell("defs").unwrap_err();
        assert!(
            failure.to_string().contains("CGNK011") || failure.code == "CGNK004",
            "{failure}"
        );
        let saved = write_notebook(k.notebook()).unwrap();
        let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
        assert_eq!(reopened.status("other").unwrap(), CellStatus::Cached);
    }
}

#[test]
fn contract_forward_references_and_hidden_unknown_effects_fail_closed() {
    let forward = NotebookKernel::open(Notebook {
        version: 1,
        cells: vec![
            code("defs", "fn f() -> i64:\n    requires later()\n    0\n"),
            code("later", "fn later() -> Bool:\n    true\n"),
        ],
    })
    .err()
    .unwrap();
    assert_eq!(forward.code, "CGNK009");
    for contract in [
        "requires unknown()",
        "ensures unknown()",
        "requires false == unknown()",
        "ensures host.read()",
    ] {
        let defs = format!("fn f() -> i64:\n    {contract}\n    0\n");
        let mut k = kernel(vec![code("defs", &defs), code("other", "42")], false);
        // An unrun, unknown call in a contract must be opaque across save/reopen.
        k.rerun_cell("other").unwrap();
        let saved = write_notebook(k.notebook()).unwrap();
        let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
        assert_eq!(reopened.status("other").unwrap(), CellStatus::Stale);
        let failure = k.rerun_cell("defs").unwrap_err();
        assert_eq!(failure.code, "CGNK011", "{contract}: {failure}");
        // A checked admission failure proves no effect ran; unchanged failed
        // source must not poison an independent result after an explicit rerun.
        k.rerun_cell("other").unwrap();
        let saved = write_notebook(k.notebook()).unwrap();
        let reopened = NotebookKernel::open(parse_notebook(&saved).unwrap()).unwrap();
        assert_eq!(reopened.status("other").unwrap(), CellStatus::Cached);
    }
}

#[test]
fn checker_rejects_non_boolean_contracts_result_misuse_and_effectful_helpers() {
    for source in [
        "fn f() -> i64:\n    requires result == 0\n    0\n",
        "fn f(result: i64) -> i64:\n    ensures true\n    result\n",
        "fn f() -> i64:\n    requires 1\n    0\n",
        "fn f() -> i64:\n    ensures n == 0\n    let n = 0\n    n\n",
        "fn f(x: String) -> i64:\n    ensures true\n    0\n",
        "fn f() -> i64:\n    requires arg_count() == 0\n    0\n",
        "fn mutating() -> Bool effects(Mutate):\n    let mut n = 0\n    n += 1\n    true\nfn f() -> i64:\n    requires mutating()\n    0\n",
        "fn f() -> i64:\n    ensures mutate()\n    0\nfn mutate() -> Bool effects(IO):\n    true\n",
        "fn f() -> i64:\n    requires captured > 0\n    0\n",
    ] {
        let mut k = kernel(vec![code("captured", "let captured = 1"), code("defs", source)], false);
        k.run_cell("captured").unwrap();
        let failure = k.run_cell("defs").unwrap_err();
        assert_eq!(failure.cell_id.as_deref(), Some("defs"), "{source}");
    }
}

#[test]
fn contracts_obey_deadlines_and_recover_without_partial_bindings() {
    let mut k = kernel(
        vec![
            code("defs", SUM),
            code("call", "let pending = 1\nlet answer = sum(2)"),
            code("read", "answer"),
        ],
        true,
    );
    k.run_cell("defs").unwrap();
    k.set_execution_control(ExecutionControl::new(
        PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
        Duration::ZERO,
    ));
    assert_eq!(k.run_cell("call").unwrap_err().code, "CGNK010");
    assert!(k.rerun_cell("read").is_err());
    k.set_execution_control(ExecutionControl::new(
        PathBuf::from(env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker")),
        Duration::from_secs(2),
    ));
    k.rerun_cell("call").unwrap();
    k.rerun_cell("read").unwrap();
    assert_eq!(output(&k, "read"), "8");
}

#[test]
fn imported_contract_predicates_lower_and_carry_transitive_provenance() {
    for worker in [false, true] {
        let project = Project::new();
        let book = parse_notebook(include_str!(
            "../../cgx2-notebook-cli/examples/contract-imports/contracts.cgnb"
        ))
        .unwrap();
        let mut k = project.open(book, worker);
        k.restart_and_run_all().unwrap();
        assert_eq!(output(&k, "value"), "8");
        for id in ["definition", "value"] {
            let package = k
                .notebook()
                .cells
                .iter()
                .find(|c| c.id == id)
                .unwrap()
                .provenance
                .as_ref()
                .unwrap()
                .package
                .as_ref()
                .unwrap();
            assert_eq!(
                package
                    .modules
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["demo.base", "demo.guards"]
            );
        }
        let saved = write_notebook(k.notebook()).unwrap();
        let mut reopened = project.open(parse_notebook(&saved).unwrap(), worker);
        assert_eq!(reopened.status("value").unwrap(), CellStatus::Cached);
        reopened.restart_and_run_all().unwrap();
        reopened
            .replace_cell("parameter", "let x = -1".into())
            .unwrap();
        assert!(reopened
            .rerun_affected("parameter")
            .unwrap_err()
            .message
            .contains("CGR015"));
        std::fs::write(
            project.0.join("src/base.cg"),
            "module demo.base\npub fn allowed(x: i64) -> Bool effects() caps():\n    x > 2\n",
        )
        .unwrap();
        let reopened = project.open(parse_notebook(&saved).unwrap(), worker);
        assert_eq!(reopened.status("definition").unwrap(), CellStatus::Stale);
        assert_eq!(reopened.status("value").unwrap(), CellStatus::Stale);
        assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
        assert!(k.rerun_cell("value").is_err());
        assert_eq!(k.status("independent").unwrap(), CellStatus::Current);
    }
}

#[test]
fn imported_functions_keep_their_frozen_contract_and_effect_rejection_profile() {
    for source in [
        "module demo.base\npub fn allowed(x: i64) -> Bool effects() caps():\n    requires x >= 0\n    true\n",
        "module demo.base\npub fn allowed(x: i64) -> Bool effects(Mutate) caps():\n    let mut n = x\n    n += 1\n    n > 0\n",
        "module demo.base\npub fn allowed(x: i64) -> Bool effects(IO):\n    true\n",
    ] {
        let project = Project::new();
        std::fs::write(project.0.join("src/base.cg"), source).unwrap();
        let book = parse_notebook(include_str!("../../cgx2-notebook-cli/examples/contract-imports/contracts.cgnb")).unwrap();
        let failure = match NotebookKernel::open_with_project(book, Some(&project.0)) {
            Err(failure) => failure,
            Ok(mut k) => k.run_cell("imports").unwrap_err(),
        };
        assert_eq!(failure.code, "CGNK012", "{source}");
    }
}

#[test]
fn contract_diagnostics_use_source_spans_and_match_direct_and_worker() {
    let mut failures = Vec::new();
    for worker in [false, true] {
        let mut k = kernel(
            vec![
                code("defs", SUM),
                code("call", "let pending = 1\nlet answer = sum(-1)"),
            ],
            worker,
        );
        k.run_cell("defs").unwrap();
        let failure = k.run_cell("call").unwrap_err();
        assert!(failure.message.contains("at 16..36"), "{failure}");
        failures.push(failure);
        let source = "fn f(x: i64) -> i64:\n    ensures 1\n    x\n";
        let mut k = kernel(vec![code("defs", source)], worker);
        let failure = k.run_cell("defs").unwrap_err();
        let start = source.find("ensures 1").unwrap() + "ensures ".len();
        assert!(
            failure
                .message
                .contains(&format!("at {start}..{}", start + 1)),
            "{failure}"
        );
    }
    assert_eq!(failures[0], failures[1]);
}

#[test]
fn contract_cost_overrun_rejects_before_worker_and_before_definition_publication() {
    let mut k = kernel(vec![code("defs", "fn guard() -> Bool:\n    for i in range(0, 33333):\n        0\n    true\nfn f() -> i64:\n    requires guard()\n    0\n"), code("value", "f()")], false);
    k.set_execution_control(ExecutionControl::new(
        PathBuf::from("/nonexistent-contract-worker"),
        Duration::from_secs(2),
    ));
    assert_eq!(k.run_cell("defs").unwrap_err().code, "CGNK006");
    assert!(k.rerun_cell("value").is_err());
}
