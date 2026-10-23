use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use cgx2_notebook_format::{
    export_document, parse_notebook, parse_self_contained, write_notebook, Cell, CellKind,
    Notebook, Output,
};
use cgx2_notebook_kernel::{facts_for_source, ExecutionControl, NotebookKernel};
use cgx2_notebook_session::CellStatus;

static NEXT: AtomicU64 = AtomicU64::new(0);
const ALPHA: &str = include_str!("../../cgx2-notebook-cli/examples/public-records/src/alpha.cg");
const BETA: &str = include_str!("../../cgx2-notebook-cli/examples/public-records/src/beta.cg");
const MANIFEST: &str = include_str!("../../cgx2-notebook-cli/examples/public-records/cgx2.toml");
const EXAMPLE: &str =
    include_str!("../../cgx2-notebook-cli/examples/public-records/public-records.cgnb");
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-public-records-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let project = Self(path);
        project.write("cgx2.toml", MANIFEST);
        project.write("src/alpha.cg", ALPHA);
        project.write("src/beta.cg", BETA);
        project
    }
    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn open(&self, book: Notebook) -> NotebookKernel {
        NotebookKernel::open_with_project(book, Some(&self.0)).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn code(id: &str, source: &str) -> Cell {
    Cell {
        id: id.into(),
        kind: CellKind::Code,
        source: source.into(),
        outputs: vec![],
        provenance: None,
    }
}
fn book(cells: Vec<Cell>) -> Notebook {
    Notebook { version: 1, cells }
}
fn output(kernel: &NotebookKernel, id: &str) -> String {
    let cell = kernel
        .notebook()
        .cells
        .iter()
        .find(|cell| cell.id == id)
        .unwrap();
    let Output::Text { body, .. } = &cell.outputs[0] else {
        panic!("text expected")
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
fn imported_local_and_same_named_module_records_survive_workers_replay_and_export() {
    let project = Project::new();
    for worker in [false, true] {
        let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
        if worker {
            kernel.set_execution_control(control());
        }
        kernel.restart_and_run_all().unwrap();
        assert_eq!(
            output(&kernel, "value"),
            "demo.alpha.Point(x: 42, active: true)"
        );
        assert_eq!(output(&kernel, "read"), "42");
        assert_eq!(output(&kernel, "beta"), "14");
        assert_eq!(output(&kernel, "local_value"), "Point(x: 9, active: false)");
        let independent = kernel.notebook().cells.last().unwrap().provenance.clone();
        kernel
            .replace_cell("parameter", "let delta = 2".into())
            .unwrap();
        kernel.rerun_affected("parameter").unwrap();
        assert_eq!(output(&kernel, "read"), "43");
        assert_eq!(
            kernel.notebook().cells.last().unwrap().provenance,
            independent
        );
        kernel.rerun_cell("read").unwrap();
        assert_eq!(output(&kernel, "read"), "43");
        let exported = export_document(kernel.notebook(), &Default::default()).unwrap();
        let document = parse_self_contained(&exported).unwrap();
        assert!(NotebookKernel::open(document.notebook.clone())
            .unwrap_err()
            .message
            .contains("--project"));
        let mut reopened = project.open(document.notebook);
        assert_eq!(reopened.status("read").unwrap(), CellStatus::Cached);
        if worker {
            reopened.set_execution_control(control());
        }
        reopened.restart_and_run_all().unwrap();
        assert_eq!(output(&reopened, "read"), "43");
    }
}

#[test]
fn annotation_and_inactive_branch_edges_preserve_independent_outputs() {
    let project = Project::new();
    let mut kernel = project.open(book(vec![
        code("alpha", "import demo.alpha"), code("beta", "import demo.beta"),
        code("annotation", "fn unused(p: alpha.Point) -> i64:\n    7"),
        code("branch", "fn choose(flag: Bool) -> i64:\n    if flag:\n        return 42\n    else:\n        return beta.value(beta.Point(active: false, x: 7))"),
        code("call", "choose(true)"), code("independent", "42"),
    ]));
    kernel.restart_and_run_all().unwrap();
    let saved = write_notebook(kernel.notebook()).unwrap();
    project.write(
        "src/alpha.cg",
        &ALPHA.replace(
            "    x: i64\n    active: Bool",
            "    active: Bool\n    x: i64",
        ),
    );
    let reopened = project.open(parse_notebook(&saved).unwrap());
    assert_eq!(reopened.status("annotation").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("call").unwrap(), CellStatus::Cached);
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
    project.write("src/beta.cg", &BETA.replace("p.x * 2", "p.x * 3"));
    let mut reopened = project.open(parse_notebook(&saved).unwrap());
    assert_eq!(reopened.status("branch").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("call").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
    reopened.restart_and_run_all().unwrap();
    assert_eq!(output(&reopened, "call"), "42");
    let facts = facts_for_source("fn ignored(p: demo.alpha.Point) -> i64:\n    1").unwrap();
    assert!(facts.reads.contains("demo.alpha"));
}

#[test]
fn wrong_nominal_arguments_fail_atomically_and_can_recover() {
    let project = Project::new();
    for worker in [false, true] {
        for wrong in [
            "beta.Point(active: true, x: 4)",
            "Point(active: true, x: 4)",
        ] {
            let mut kernel = project.open(book(vec![
                code("imports", "import demo.alpha\nimport demo.beta"),
                code("local", "record Point:\n    x: i64\n    active: Bool"),
                code("stable", "let stable = alpha.make(5)"),
                code(
                    "bad",
                    &format!("let pending = alpha.make(6)\nalpha.value({wrong})"),
                ),
                code("read", "alpha.value(stable)"),
            ]));
            if worker {
                kernel.set_execution_control(control());
            }
            for id in ["imports", "local", "stable"] {
                kernel.run_cell(id).unwrap();
            }
            let error = kernel.run_cell("bad").unwrap_err();
            assert_eq!(error.cell_id.as_deref(), Some("bad"));
            assert!(error.message.contains("at "), "{error}");
            kernel.replace_cell("bad", "pending.x".into()).unwrap();
            assert!(kernel
                .rerun_cell("bad")
                .unwrap_err()
                .message
                .contains("pending"));
            kernel
                .replace_cell("bad", "alpha.value(stable)".into())
                .unwrap();
            kernel.rerun_cell("bad").unwrap();
            assert_eq!(output(&kernel, "bad"), "5");
        }
    }
}

#[test]
fn changed_package_root_manifest_visibility_delete_and_stale_live_metadata_fail_closed() {
    let project = Project::new();
    let other = Project::new();
    let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
    kernel.restart_and_run_all().unwrap();
    let saved = write_notebook(kernel.notebook()).unwrap();
    let reopened = other.open(parse_notebook(&saved).unwrap());
    assert_eq!(reopened.status("read").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
    project.write("cgx2.toml", &format!("{MANIFEST}\n# changed input\n"));
    let reopened = project.open(parse_notebook(&saved).unwrap());
    assert_eq!(reopened.status("beta").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
    assert!(kernel
        .rerun_cell("read")
        .unwrap_err()
        .message
        .contains("CGP018"));
    project.write("cgx2.toml", MANIFEST);
    project.write(
        "src/alpha.cg",
        &ALPHA.replace("pub record Point", "record Point"),
    );
    assert!(
        NotebookKernel::open_with_project(parse_notebook(&saved).unwrap(), Some(&project.0))
            .is_err()
    );
    fs::remove_file(project.0.join("src/alpha.cg")).unwrap();
    assert!(
        NotebookKernel::open_with_project(parse_notebook(&saved).unwrap(), Some(&project.0))
            .is_err()
    );
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
}

#[test]
fn qualified_annotations_constructors_unknown_private_and_ambiguous_roots() {
    let project = Project::new();
    let mut kernel = project.open(book(vec![code("imports", "import demo.alpha"),
        code("function", "fn identity(p: demo.alpha.Point) -> demo.alpha.Point:\n    let q: demo.alpha.Point = p\n    q"),
        code("value", "let p: demo.alpha.Point = identity(demo.alpha.Point(active: true, x: 3))\np.x")]));
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output(&kernel, "value"), "3");
    for source in [
        "alpha.Missing(x: 1)",
        "fn bad(p: alpha.Missing) -> i64:\n    1",
        "fn bad(p: missing.Point) -> i64:\n    1",
        "let alpha = 7",
        "fn bad(alpha: alpha.Point) -> i64:\n    1",
        "fn bad(p: alpha.Point) -> i64:\n    let alpha = p\n    1",
        "record alpha:\n    x: i64",
    ] {
        let error = NotebookKernel::open_with_project(
            book(vec![
                code("imports", "import demo.alpha"),
                code("bad", source),
            ]),
            Some(&project.0),
        )
        .unwrap_err();
        assert!(
            error.message.contains("bad") && error.message.contains("at "),
            "{error}"
        );
    }
    assert!(NotebookKernel::open_with_project(
        book(vec![code("bad", "alpha.Point(x: 1, active: true)")]),
        Some(&project.0)
    )
    .is_err());
    project.write(
        "src/alpha.cg",
        &format!("{ALPHA}\nrecord Secret:\n    x: i64\n"),
    );
    assert!(NotebookKernel::open_with_project(
        book(vec![
            code("imports", "import demo.alpha"),
            code("bad", "alpha.Secret(x: 1)")
        ]),
        Some(&project.0)
    )
    .is_err());
}

#[test]
fn imported_constructor_order_checked_arithmetic_and_failure_recovery() {
    let project = Project::new();
    for worker in [false, true] {
        let mut kernel = project.open(book(vec![
            code("import", "import demo.alpha"),
            code(
                "bad",
                "alpha.Point(active: 1 / 0 == 0, x: 9223372036854775807 + 1)",
            ),
        ]));
        if worker {
            kernel.set_execution_control(control());
        }
        kernel.run_cell("import").unwrap();
        assert!(kernel
            .run_cell("bad")
            .unwrap_err()
            .message
            .contains("division by zero"));
        kernel
            .replace_cell(
                "bad",
                "alpha.Point(x: 9223372036854775807 + 1, active: 1 / 0 == 0)".into(),
            )
            .unwrap();
        assert!(kernel
            .rerun_cell("bad")
            .unwrap_err()
            .message
            .contains("overflow"));
        kernel
            .replace_cell(
                "bad",
                "alpha.Point(active: false, x: 0 - 9223372036854775807 - 1)".into(),
            )
            .unwrap();
        kernel.rerun_cell("bad").unwrap();
        assert_eq!(
            output(&kernel, "bad"),
            "demo.alpha.Point(x: -9223372036854775808, active: false)"
        );
    }
}

#[test]
fn unicode_module_record_fields_and_generated_name_collisions() {
    let project = Project::new();
    project.write("src/alpha.cg", "module demo.座標\npub record 点:\n    位置: i64\n    有効: Bool\npub fn 作成(x: i64) -> 点 effects() caps():\n    点(有効: true, 位置: x)\n");
    for worker in [false, true] {
        let mut kernel = project.open(book(vec![
            code("import", "import demo.座標"),
            code("value", "let p = 座標.作成(7)\np"),
            code("read", "p.位置"),
        ]));
        if worker {
            kernel.set_execution_control(control());
        }
        kernel.restart_and_run_all().unwrap();
        assert_eq!(
            output(&kernel, "value"),
            "demo.座標.点(位置: 7, 有効: true)"
        );
        assert_eq!(output(&kernel, "read"), "7");
        let saved = write_notebook(kernel.notebook()).unwrap();
        assert_eq!(
            project
                .open(parse_notebook(&saved).unwrap())
                .status("read")
                .unwrap(),
            CellStatus::Cached
        );
    }
    let error = NotebookKernel::open_with_project(
        book(vec![code(
            "bad",
            "record __cgx2_pkg_4demo_5alpha_5Point:\n    x: i64",
        )]),
        Some(&project.0),
    )
    .unwrap_err();
    assert!(error.message.contains("reserved"));
    // Existing local notebooks may use these names: worker transport must not reinterpret them.
    let mut local = NotebookKernel::open(book(vec![code("local", "record __cgx2_pkg_4demo_5alpha_5Point:\n    x: i64\nlet p = __cgx2_pkg_4demo_5alpha_5Point(x: 7)\np")])).unwrap();
    local.set_execution_control(control());
    local.restart_and_run_all().unwrap();
    assert_eq!(
        output(&local, "local"),
        "__cgx2_pkg_4demo_5alpha_5Point(x: 7)"
    );
}

#[cfg(unix)]
#[test]
fn worker_tampering_cannot_publish_a_different_valid_nominal_record() {
    use std::os::unix::fs::PermissionsExt;
    let project = Project::new();
    for identity in [
        "demo.beta.Point",
        "demo.unknown.Point",
        "Point",
        "demo.alpha.Point",
    ] {
        let mut frame = vec![b'R'];
        frame.extend_from_slice(&(identity.len() as u16).to_be_bytes());
        frame.extend_from_slice(identity.as_bytes());
        frame.push(2);
        // Last case has a schema mismatch despite the expected nominal identity.
        for (field, tag, data) in [
            ("x", b'I', 7_i64.to_be_bytes().to_vec()),
            (
                "active",
                b'B',
                vec![if identity == "demo.alpha.Point" { 2 } else { 1 }],
            ),
        ] {
            frame.extend_from_slice(&(field.len() as u16).to_be_bytes());
            frame.extend_from_slice(field.as_bytes());
            frame.push(tag);
            frame.extend(data);
        }
        let frame_path = project.0.join("frame");
        fs::write(&frame_path, frame).unwrap();
        let worker = project.0.join("worker");
        fs::write(
            &worker,
            format!(
                "#!/bin/sh\ncat > /dev/null\ncat '{}'\n",
                frame_path.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&worker, fs::Permissions::from_mode(0o700)).unwrap();
        let mut kernel = project.open(book(vec![
            code("imports", "import demo.alpha\nimport demo.beta"),
            code("stable", "let stable = 42"),
            code("bad", "let poison = alpha.make(7)"),
            code("read", "stable"),
        ]));
        kernel.run_cell("imports").unwrap();
        kernel.run_cell("stable").unwrap();
        kernel.set_execution_control(ExecutionControl::new(worker, Duration::from_secs(2)));
        let error = kernel.run_cell("bad").unwrap_err();
        assert_eq!(error.code, "CGNK010", "{error}");
        assert_eq!(error.cell_id.as_deref(), Some("bad"));
        kernel.set_execution_control(control());
        kernel.replace_cell("bad", "poison.x".into()).unwrap();
        assert!(kernel.rerun_cell("bad").is_err());
        kernel.replace_cell("bad", "stable".into()).unwrap();
        kernel.rerun_cell("bad").unwrap();
        assert_eq!(output(&kernel, "bad"), "42");
    }
}

#[test]
fn live_module_changes_discard_only_affected_definitions_before_replay() {
    for mutation in ["edit", "delete", "visibility", "fields", "manifest"] {
        let project = Project::new();
        let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
        kernel.restart_and_run_all().unwrap();
        let independent = kernel.notebook().cells.last().unwrap().clone();
        match mutation {
            "edit" => project.write("src/alpha.cg", &ALPHA.replace("p.x + 1", "p.x + 2")),
            "delete" => fs::remove_file(project.0.join("src/alpha.cg")).unwrap(),
            "visibility" => project.write("src/alpha.cg", &ALPHA.replace("pub record", "record")),
            "fields" => project.write(
                "src/alpha.cg",
                &ALPHA.replace(
                    "    x: i64\n    active: Bool",
                    "    active: Bool\n    x: i64",
                ),
            ),
            "manifest" => project.write("cgx2.toml", &format!("{MANIFEST}\n# changed\n")),
            _ => unreachable!(),
        }
        let error = kernel.rerun_cell("read").unwrap_err();
        assert!(
            error.message.contains("CGP") && error.message.contains("at "),
            "{mutation}: {error}"
        );
        for id in ["alpha_import", "function", "value", "read"] {
            assert_eq!(
                kernel.status(id).unwrap(),
                CellStatus::Stale,
                "{mutation}: {id}"
            );
        }
        assert_eq!(
            kernel.status("beta").unwrap(),
            if mutation == "manifest" {
                CellStatus::Stale
            } else {
                CellStatus::Current
            }
        );
        assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
        assert_eq!(kernel.notebook().cells.last().unwrap(), &independent);
        assert!(kernel.rerun_cell("read").is_err());
    }
}

#[test]
fn removing_imports_and_effect_failures_do_not_replay_hidden_work() {
    let project = Project::new();
    let mut kernel = project.open(book(vec![
        code("imports", "import demo.alpha"),
        code("value", "let p = alpha.make(7)"),
        code("read", "p.x"),
        code("independent", "42"),
    ]));
    kernel.restart_and_run_all().unwrap();
    kernel.remove_cell("imports").unwrap();
    assert_eq!(kernel.status("value").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("read").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
    assert!(kernel.rerun_cell("read").is_err());
    let mut kernel = project.open(book(vec![
        code("imports", "import demo.alpha"),
        code("value", "let p = alpha.make(7)"),
        code("effect", "unknown_host_effect()"),
        code("read", "p.x"),
    ]));
    kernel.run_cell("imports").unwrap();
    kernel.run_cell("value").unwrap();
    assert!(kernel.run_cell("effect").is_err());
    kernel.rerun_cell("read").unwrap();
    assert_eq!(output(&kernel, "read"), "7");
    assert_eq!(kernel.status("effect").unwrap(), CellStatus::Failed);
    let control = control();
    control.interrupt();
    kernel.set_execution_control(control);
    assert_eq!(kernel.rerun_cell("read").unwrap_err().code, "CGNK010");
    kernel.set_execution_control(ExecutionControl::new(
        env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker").into(),
        Duration::ZERO,
    ));
    assert_eq!(kernel.rerun_cell("read").unwrap_err().code, "CGNK010");
    kernel.set_execution_control(self::control());
    kernel.rerun_cell("read").unwrap();
    assert_eq!(output(&kernel, "read"), "7");
}

#[test]
fn older_package_capture_without_root_is_readable_but_stale_and_duplicate_roots_reject() {
    let project = Project::new();
    let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
    kernel.restart_and_run_all().unwrap();
    let saved = write_notebook(kernel.notebook()).unwrap();
    let legacy = saved
        .lines()
        .map(|line| {
            line.split_whitespace()
                .find(|part| part.starts_with("root_sha256="))
                .map_or_else(
                    || line.to_owned(),
                    |root| line.replace(&format!(" {root}"), ""),
                )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let reopened = project.open(parse_notebook(&format!("{legacy}\n")).unwrap());
    assert_eq!(reopened.status("read").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
    let root = saved
        .split_whitespace()
        .find(|part| part.starts_with("root_sha256="))
        .unwrap();
    assert!(parse_notebook(&saved.replacen(root, &format!("{root} {root}"), 1)).is_err());
    assert!(parse_notebook(&saved.replacen(root, "root_sha256=invalid", 1)).is_err());
}

#[cfg(unix)]
#[test]
fn same_bytes_at_a_changed_canonical_module_target_invalidate_before_replay() {
    use std::os::unix::fs::symlink;
    let project = Project::new();
    let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
    kernel.restart_and_run_all().unwrap();
    project.write("src/replacement.cg", ALPHA);
    fs::remove_file(project.0.join("src/alpha.cg")).unwrap();
    symlink("replacement.cg", project.0.join("src/alpha.cg")).unwrap();
    assert!(kernel
        .rerun_cell("read")
        .unwrap_err()
        .message
        .contains("target changed"));
    assert_eq!(kernel.status("read").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("beta").unwrap(), CellStatus::Current);
}

#[test]
fn diagnostics_refer_to_original_cell_bytes_after_qualified_names_expand() {
    let project = Project::new();
    for worker in [false, true] {
        let source = "let p = alpha.Point(active: true, x: 1)\nalpha.value(p) / 0";
        let mut kernel = project.open(book(vec![
            code("import", "import demo.alpha"),
            code("bad", source),
        ]));
        if worker {
            kernel.set_execution_control(control());
        }
        kernel.run_cell("import").unwrap();
        let error = kernel.run_cell("bad").unwrap_err();
        let start = source.find("alpha.value").unwrap();
        assert_eq!(error.cell_id.as_deref(), Some("bad"));
        assert!(
            error
                .message
                .contains(&format!("at {start}..{}", source.len())),
            "{error}"
        );
    }
}
