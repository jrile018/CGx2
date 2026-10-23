use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use cgx2_notebook_format::{parse_notebook, write_notebook, Cell, CellKind, Notebook, Output};
use cgx2_notebook_kernel::{ExecutionControl, NotebookKernel};
use cgx2_notebook_session::CellStatus;

const EXAMPLE: &str =
    include_str!("../../cgx2-notebook-cli/examples/import-closure/import-closure.cgnb");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-closure-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let project = Self(root);
        for (name, source) in [
            (
                "cgx2.toml",
                include_str!("../../cgx2-notebook-cli/examples/import-closure/cgx2.toml"),
            ),
            (
                "src/api.cg",
                include_str!("../../cgx2-notebook-cli/examples/import-closure/src/api.cg"),
            ),
            (
                "src/left.cg",
                include_str!("../../cgx2-notebook-cli/examples/import-closure/src/left.cg"),
            ),
            (
                "src/right.cg",
                include_str!("../../cgx2-notebook-cli/examples/import-closure/src/right.cg"),
            ),
            (
                "src/base.cg",
                include_str!("../../cgx2-notebook-cli/examples/import-closure/src/base.cg"),
            ),
            (
                "src/other.cg",
                include_str!("../../cgx2-notebook-cli/examples/import-closure/src/other.cg"),
            ),
            (
                "src/unrelated.cg",
                include_str!("../../cgx2-notebook-cli/examples/import-closure/src/unrelated.cg"),
            ),
        ] {
            project.write(name, source);
        }
        project
    }
    fn write(&self, path: &str, source: &str) {
        fs::write(self.0.join(path), source).unwrap();
    }
    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.0.join(path)).unwrap()
    }
    fn open(&self, book: Notebook) -> NotebookKernel {
        NotebookKernel::open_with_project(book, Some(&self.0)).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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
    match &cell.outputs[0] {
        Output::Text { body, .. } => body.clone(),
        _ => panic!("text output expected"),
    }
}
fn control() -> ExecutionControl {
    ExecutionControl::new(
        env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker").into(),
        Duration::from_secs(5),
    )
}
fn modules(kernel: &NotebookKernel, id: &str) -> Vec<String> {
    kernel
        .notebook()
        .cells
        .iter()
        .find(|cell| cell.id == id)
        .unwrap()
        .provenance
        .as_ref()
        .unwrap()
        .package
        .as_ref()
        .unwrap()
        .modules
        .keys()
        .cloned()
        .collect()
}

#[test]
fn chain_diamond_shared_roots_records_and_persistence() {
    let project = Project::new();
    for worker in [false, true] {
        let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
        if worker {
            kernel.set_execution_control(control());
        }
        kernel.restart_and_run_all().unwrap();
        assert_eq!(
            output(&kernel, "value"),
            "demo.base.Point(x: 41, active: true)"
        );
        assert_eq!(output(&kernel, "read"), "41");
        assert_eq!(output(&kernel, "call"), "41");
        assert_eq!(
            output(&kernel, "other"),
            "demo.other.Point(x: 7, active: false)"
        );
        for id in ["api_import", "function", "value", "read", "call"] {
            assert_eq!(
                modules(&kernel, id),
                ["demo.api", "demo.base", "demo.left", "demo.right"]
            );
        }
        let independent = kernel.notebook().cells.last().unwrap().clone();
        kernel
            .replace_cell("parameter", "let input = 41".into())
            .unwrap();
        kernel.rerun_affected("parameter").unwrap();
        assert_eq!(output(&kernel, "read"), "42");
        assert_eq!(kernel.notebook().cells.last().unwrap(), &independent);
        let saved = write_notebook(kernel.notebook()).unwrap();
        let mut reopened = project.open(parse_notebook(&saved).unwrap());
        assert_eq!(reopened.status("read").unwrap(), CellStatus::Cached);
        reopened.set_execution_control(control());
        reopened.restart_and_run_all().unwrap();
        assert_eq!(output(&reopened, "call"), "42");
    }
    // Two explicit roots share base; neither base definitions nor identities duplicate.
    let mut kernel = project.open(book(vec![
        code("imports", "import demo.left\nimport demo.right"),
        code("call", "let p = left.make(41)\nright.value(p)"),
    ]));
    kernel.set_execution_control(control());
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output(&kernel, "call"), "42");
    assert_eq!(
        modules(&kernel, "call"),
        ["demo.base", "demo.left", "demo.right"]
    );
}

#[test]
fn internal_modules_are_not_source_imports_but_inferred_records_are_retained() {
    let project = Project::new();
    for source in [
        "base.make(1)", "base.Point(x: 1, active: true)",
        "demo.base.Point(x: 1, active: true)", "demo.base.make(1)",
        "let p: base.Point = api.make(1)", "let p: demo.base.Point = api.make(1)",
        "fn f(p: demo.base.Point) -> i64 effects() caps():\n    p.x",
        "fn f(x: i64) -> i64 effects() caps():\n    if true:\n        x\n    else:\n        base.value(api.make(x))",
    ] {
        let error = NotebookKernel::open_with_project(book(vec![code("import", "import demo.api"), code("bad", source)]), Some(&project.0)).unwrap_err();
        assert!(error.message.contains("bad") && error.message.contains("at "), "{source}: {error}");
    }
    let mut kernel = project.open(book(vec![
        code("import", "import demo.api"),
        code("other", "import demo.other"),
        code("value", "let p = api.make(1)"),
        code("field", "p.x"),
        code("base_import", "import demo.base"),
        code("typed", "let q: base.Point = p\nbase.value(q)"),
    ]));
    kernel.set_execution_control(control());
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output(&kernel, "field"), "2");
    assert_eq!(output(&kernel, "typed"), "2");
    // A same-shaped record with another module owner cannot cross this API.
    kernel
        .replace_cell("typed", "api.value(other.make(1))".into())
        .unwrap();
    assert!(kernel.rerun_cell("typed").is_err());
}

#[test]
fn transitive_snapshot_edits_selectively_discard_live_and_saved_state() {
    for mutation in [
        "source",
        "fields",
        "visibility",
        "delete",
        "edge",
        "canonical",
        "manifest",
        "root",
    ] {
        let project = Project::new();
        let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
        kernel.restart_and_run_all().unwrap();
        let saved = write_notebook(kernel.notebook()).unwrap();
        let independent = kernel.notebook().cells.last().unwrap().clone();
        match mutation {
            "source" => project.write(
                "src/base.cg",
                &project.read("src/base.cg").replace("x + 1", "x + 2"),
            ),
            "fields" => project.write(
                "src/base.cg",
                &project.read("src/base.cg").replace(
                    "    x: i64\n    active: Bool",
                    "    active: Bool\n    x: i64",
                ),
            ),
            "visibility" => project.write(
                "src/base.cg",
                &project.read("src/base.cg").replace("pub record", "record"),
            ),
            "delete" => fs::remove_file(project.0.join("src/base.cg")).unwrap(),
            "edge" => project.write(
                "src/left.cg",
                &project
                    .read("src/left.cg")
                    .replace("import demo.base", "import demo.base\nimport demo.other"),
            ),
            "canonical" => {
                #[cfg(unix)]
                {
                    fs::rename(
                        project.0.join("src/base.cg"),
                        project.0.join("src/replacement.cg"),
                    )
                    .unwrap();
                    std::os::unix::fs::symlink("replacement.cg", project.0.join("src/base.cg"))
                        .unwrap();
                }
            }
            "manifest" => project.write(
                "cgx2.toml",
                &format!("{}\n# edit\n", project.read("cgx2.toml")),
            ),
            "root" => {
                fs::rename(&project.0, project.0.with_extension("moved")).unwrap();
                #[cfg(unix)]
                std::os::unix::fs::symlink(project.0.with_extension("moved"), &project.0).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(kernel.rerun_cell("read").is_err(), "{mutation}");
        for id in ["api_import", "function", "value", "read", "call"] {
            assert_eq!(
                kernel.status(id).unwrap(),
                CellStatus::Stale,
                "{mutation}: {id}"
            );
        }
        assert_eq!(
            kernel.status("independent").unwrap(),
            CellStatus::Current,
            "{mutation}"
        );
        assert_eq!(kernel.notebook().cells.last().unwrap(), &independent);
        if !matches!(mutation, "manifest" | "root") {
            assert_eq!(
                kernel.status("other").unwrap(),
                CellStatus::Current,
                "{mutation}"
            );
        }
        if matches!(
            mutation,
            "source" | "fields" | "edge" | "canonical" | "manifest" | "root"
        ) {
            let reopened = project.open(parse_notebook(&saved).unwrap());
            assert_eq!(
                reopened.status("read").unwrap(),
                CellStatus::Stale,
                "{mutation}"
            );
            assert_eq!(
                reopened.status("independent").unwrap(),
                CellStatus::Cached,
                "{mutation}"
            );
        }
        if mutation == "root" {
            fs::remove_file(&project.0).unwrap();
            fs::rename(project.0.with_extension("moved"), &project.0).unwrap();
        }
    }
    let project = Project::new();
    let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
    kernel.restart_and_run_all().unwrap();
    let saved = write_notebook(kernel.notebook()).unwrap();
    project.write(
        "src/unrelated.cg",
        &project.read("src/unrelated.cg").replace("42", "43"),
    );
    let reopened = project.open(parse_notebook(&saved).unwrap());
    assert_eq!(reopened.status("read").unwrap(), CellStatus::Cached);
    // Live frozen snapshot still requires reopen; unrelated outputs remain valid.
    assert!(kernel.rerun_cell("read").is_err());
    assert_eq!(kernel.status("read").unwrap(), CellStatus::Current);
}

#[test]
fn unused_declared_and_inactive_dependencies_are_checked_and_tracked() {
    let project = Project::new();
    // Inactive calls still require the record owner under frozen package checking.
    project.write("src/api.cg", "module demo.api\nimport demo.left\nimport demo.right\nimport demo.base\npub fn answer(x: i64) -> i64 caps():\n    if true:\n        return x\n    else:\n        return demo.right.value(demo.left.make(x))\n");
    let notebook = book(vec![
        code("import", "import demo.api"),
        code("call", "api.answer(42)"),
        code("independent", "42"),
    ]);
    let mut kernel = project.open(notebook.clone());
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output(&kernel, "call"), "42");
    assert_eq!(
        modules(&kernel, "call"),
        ["demo.api", "demo.base", "demo.left", "demo.right"]
    );
    // The import is unused, and its function is never called, but its effects reject it.
    project.write(
        "src/api.cg",
        &project.read("src/api.cg").replace(
            "import demo.left",
            "import demo.left\nimport demo.unrelated",
        ),
    );
    for declaration in [
        "pub fn hidden() -> i64 effects(ManagedAlloc) caps():\n    1\n",
        "pub fn hidden(x: i64) -> i64 effects() caps():\n    requires x > 0\n    x\n",
    ] {
        project.write(
            "src/unrelated.cg",
            &format!("module demo.unrelated\n{declaration}"),
        );
        assert!(NotebookKernel::open_with_project(notebook.clone(), Some(&project.0)).is_err());
    }
    // An unrelated unsupported module does not pollute a selected pure closure.
    project.write(
        "src/api.cg",
        &project
            .read("src/api.cg")
            .replace("import demo.unrelated\n", ""),
    );
    let mut kernel = project.open(notebook);
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output(&kernel, "call"), "42");
}

#[test]
fn failure_cancel_timeout_and_import_removal_preserve_valid_observations() {
    let project = Project::new();
    let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
    kernel.set_execution_control(control());
    kernel.restart_and_run_all().unwrap();
    let independent = kernel.notebook().cells.last().unwrap().clone();
    let cancelled = control();
    cancelled.interrupt();
    kernel.set_execution_control(cancelled);
    assert_eq!(kernel.rerun_cell("read").unwrap_err().code, "CGNK010");
    kernel.set_execution_control(ExecutionControl::new(
        env!("CARGO_BIN_EXE_cgx2-notebook-scalar-worker").into(),
        Duration::ZERO,
    ));
    assert_eq!(kernel.rerun_cell("read").unwrap_err().code, "CGNK010");
    kernel.set_execution_control(control());
    kernel.rerun_cell("read").unwrap();
    assert_eq!(output(&kernel, "read"), "41");
    assert_eq!(kernel.notebook().cells.last().unwrap(), &independent);
    kernel.remove_cell("api_import").unwrap();
    assert_eq!(kernel.status("value").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("read").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("other").unwrap(), CellStatus::Current);
    assert!(kernel.rerun_cell("read").is_err());
}

#[test]
fn annotation_only_and_unused_import_edges_have_complete_provenance() {
    let project = Project::new();
    project.write("src/left.cg", "module demo.left\nimport demo.base\npub fn tag(p: demo.base.Point) -> i64 effects() caps():\n    42\n");
    project.write(
        "src/api.cg",
        "module demo.api\nimport demo.left\npub fn answer() -> i64 effects() caps():\n    42\n",
    );
    let notebook = book(vec![
        code("import", "import demo.api"),
        code("call", "api.answer()"),
        code("independent", "42"),
    ]);
    let mut kernel = project.open(notebook);
    kernel.restart_and_run_all().unwrap();
    assert_eq!(
        modules(&kernel, "call"),
        ["demo.api", "demo.base", "demo.left"]
    );
    project.write(
        "src/base.cg",
        &project.read("src/base.cg").replace("x + 1", "x + 2"),
    );
    assert!(kernel.rerun_cell("call").is_err());
    assert_eq!(kernel.status("call").unwrap(), CellStatus::Stale);
    assert_eq!(kernel.status("independent").unwrap(), CellStatus::Current);
}

#[test]
fn combined_function_budget_counts_shared_descendants_once() {
    let project = Project::new();
    let functions = |count: usize| {
        (0..count)
            .map(|i| format!("pub fn f{i}() -> i64 effects() caps():\n    {i}\n"))
            .collect::<String>()
    };
    project.write(
        "src/base.cg",
        &format!("module demo.base\n{}", functions(16)),
    );
    project.write(
        "src/left.cg",
        &format!("module demo.left\nimport demo.base\n{}", functions(8)),
    );
    project.write(
        "src/right.cg",
        &format!("module demo.right\nimport demo.base\n{}", functions(8)),
    );
    project.write(
        "src/api.cg",
        "module demo.api\nimport demo.left\nimport demo.right\n",
    );
    let notebook = book(vec![
        code("import", "import demo.left\nimport demo.right"),
        code("call", "left.f7() + right.f7()"),
    ]);
    let mut kernel = project.open(notebook.clone());
    kernel.set_execution_control(control());
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output(&kernel, "call"), "14");
    project.write(
        "src/right.cg",
        &format!("module demo.right\nimport demo.base\n{}", functions(9)),
    );
    let error = NotebookKernel::open_with_project(notebook, Some(&project.0)).unwrap_err();
    assert!(
        error.message.contains("33 functions") && error.message.contains("lowered bytes"),
        "{error}"
    );
}

#[test]
fn closure_module_and_depth_boundary_and_combined_source_bound() {
    let project = Project::new();
    for count in [cgx2_package::MAX_SOURCES, cgx2_package::MAX_SOURCES + 1] {
        let sources = (0..count)
            .map(|i| format!("\"src/m{i}.cg\""))
            .collect::<Vec<_>>()
            .join(", ");
        project.write("cgx2.toml", &format!("schema_version = 1\nname = \"demo\"\nentry = \"src/m0.cg\"\nsources = [{sources}]\n"));
        for i in 0..count {
            let import = if i == 0 {
                String::new()
            } else {
                format!("import demo.m{}\n", i - 1)
            };
            project.write(
                &format!("src/m{i}.cg"),
                &format!("module demo.m{i}\n{import}"),
            );
        }
        let notebook = book(vec![
            code("import", &format!("import demo.m{}", count - 1)),
            code("value", "42"),
        ]);
        if count == cgx2_package::MAX_SOURCES {
            let mut kernel = project.open(notebook);
            kernel.restart_and_run_all().unwrap();
            assert_eq!(modules(&kernel, "import").len(), count);
            assert_eq!(output(&kernel, "value"), "42");
        } else {
            assert!(NotebookKernel::open_with_project(notebook, Some(&project.0)).is_err());
        }
    }
    let project = Project::new();
    let padding = "# bounded padding\n    ".repeat(30_000);
    for name in ["left", "right"] {
        project.write(
            &format!("src/{name}.cg"),
            &format!(
                "module demo.{name}\npub fn answer() -> i64 effects() caps():\n    {padding}42\n"
            ),
        );
    }
    // Remove consumers of the replaced record APIs from the package graph.
    project.write(
        "src/api.cg",
        "module demo.api\nimport demo.left\nimport demo.right\n",
    );
    let error = NotebookKernel::open_with_project(
        book(vec![code("import", "import demo.api")]),
        Some(&project.0),
    )
    .unwrap_err();
    assert!(
        error.message.contains("CGNK006")
            && error.message.contains("2 functions")
            && error.message.contains("lowered bytes"),
        "{error}"
    );
}

#[test]
fn transitive_missing_unlisted_private_cycle_and_reserved_names_fail_closed() {
    for source in [
        "module demo.base\nimport demo.absent\n",
        "module demo.base\nimport demo.api\n",
        "module demo.base\nrecord Point:\n    x: i64\n",
        "module demo.base\npub fn __cgx2_pkg_collision() -> i64 effects() caps():\n    42\n",
    ] {
        let project = Project::new();
        project.write("src/base.cg", source);
        assert!(NotebookKernel::open_with_project(
            parse_notebook(EXAMPLE).unwrap(),
            Some(&project.0)
        )
        .is_err());
    }
    let project = Project::new();
    project.write(
        "src/base.cg",
        &project
            .read("src/base.cg")
            .replace("pub fn value", "fn value"),
    );
    assert!(
        NotebookKernel::open_with_project(parse_notebook(EXAMPLE).unwrap(), Some(&project.0))
            .is_err()
    );
    let project = Project::new();
    project.write("src/unlisted.cg", "module demo.unlisted\n");
    project.write(
        "src/base.cg",
        &format!("{}\nimport demo.unlisted\n", project.read("src/base.cg")),
    );
    assert!(
        NotebookKernel::open_with_project(parse_notebook(EXAMPLE).unwrap(), Some(&project.0))
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn transitive_worker_record_identity_field_validation_is_transactional() {
    use std::os::unix::fs::PermissionsExt;
    for (identity, field) in [
        ("demo.other.Point", "x"),
        ("demo.unlisted.Point", "x"),
        ("demo.base.Point", "wrong"),
    ] {
        let project = Project::new();
        let mut frame = vec![b'R'];
        frame.extend_from_slice(&(identity.len() as u16).to_be_bytes());
        frame.extend_from_slice(identity.as_bytes());
        frame.push(2);
        for (name, tag, bytes) in [
            (field, b'I', 9_i64.to_be_bytes().to_vec()),
            ("active", b'B', vec![1]),
        ] {
            frame.extend_from_slice(&(name.len() as u16).to_be_bytes());
            frame.extend_from_slice(name.as_bytes());
            frame.push(tag);
            frame.extend(bytes);
        }
        fs::write(project.0.join("frame"), frame).unwrap();
        let worker = project.0.join("worker");
        fs::write(
            &worker,
            format!(
                "#!/bin/sh\ncat > /dev/null\ncat '{}'\n",
                project.0.join("frame").display()
            ),
        )
        .unwrap();
        fs::set_permissions(&worker, fs::Permissions::from_mode(0o700)).unwrap();
        let mut kernel = project.open(book(vec![
            code("import", "import demo.api"),
            code("stable", "let stable = 42"),
            code("bad", "let poison = api.make(1)"),
            code("read", "stable"),
        ]));
        kernel.run_cell("import").unwrap();
        kernel.run_cell("stable").unwrap();
        kernel.set_execution_control(ExecutionControl::new(worker, Duration::from_secs(2)));
        assert_eq!(kernel.run_cell("bad").unwrap_err().code, "CGNK010");
        kernel.set_execution_control(control());
        kernel.replace_cell("bad", "poison.x".into()).unwrap();
        assert!(kernel.rerun_cell("bad").is_err());
        kernel.replace_cell("bad", "stable".into()).unwrap();
        kernel.rerun_cell("bad").unwrap();
        assert_eq!(output(&kernel, "bad"), "42");
    }
}

#[test]
fn unicode_transitive_nominal_names_and_parameter_evaluation_order() {
    let project = Project::new();
    // Preserve package module spellings but exercise Unicode type/field/function names.
    for path in ["src/base.cg", "src/left.cg", "src/right.cg", "src/api.cg"] {
        project.write(
            path,
            &project
                .read(path)
                .replace("Point", "点")
                .replace("active", "有効")
                .replace("make", "作成")
                .replace("value", "値"),
        );
    }
    let mut kernel = project.open(book(vec![
        code("import", "import demo.api"),
        code("value", "let p = api.作成(41)\np"),
        code("read", "p.x"),
    ]));
    kernel.set_execution_control(control());
    kernel.restart_and_run_all().unwrap();
    assert_eq!(output(&kernel, "value"), "demo.base.点(x: 42, 有効: true)");
    assert_eq!(output(&kernel, "read"), "42");
    // Named constructor arguments preserve source order through transitive lowering.
    project.write(
        "src/base.cg",
        &project.read("src/base.cg").replace(
            "点(有効: true, x: bump(x))",
            "点(有効: 1 / 0 == 0, x: x + 1)",
        ),
    );
    let mut kernel = project.open(book(vec![
        code("import", "import demo.api"),
        code("bad", "api.作成(9223372036854775807)"),
    ]));
    kernel.set_execution_control(control());
    kernel.run_cell("import").unwrap();
    let error = kernel.run_cell("bad").unwrap_err();
    assert!(
        error.message.contains("division") || error.message.contains("zero"),
        "{error}"
    );
}

#[test]
fn canonical_module_provenance_roundtrips_and_legacy_captures_stale() {
    let project = Project::new();
    let mut kernel = project.open(parse_notebook(EXAMPLE).unwrap());
    kernel.restart_and_run_all().unwrap();
    let saved = write_notebook(kernel.notebook()).unwrap();
    let token = saved
        .split_whitespace()
        .find(|part| part.starts_with("module_path_sha256="))
        .unwrap();
    assert!(parse_notebook(&saved.replacen(token, &format!("{token} {token}"), 1)).is_err());
    let first_entry = token.split_once('=').unwrap().1.split(',').next().unwrap();
    let (name, _) = first_entry.split_once(':').unwrap();
    assert!(parse_notebook(&saved.replacen(
        token,
        &format!("module_path_sha256={name}:invalid"),
        1
    ))
    .is_err());
    assert!(parse_notebook(&saved.replacen(
        token,
        &format!("module_path_sha256={first_entry}"),
        1
    ))
    .is_err());
    let legacy = saved
        .lines()
        .map(|line| {
            line.split_whitespace()
                .find(|part| part.starts_with("module_path_sha256="))
                .map_or_else(
                    || line.to_owned(),
                    |part| line.replace(&format!(" {part}"), ""),
                )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let reopened = project.open(parse_notebook(&format!("{legacy}\n")).unwrap());
    assert_eq!(reopened.status("read").unwrap(), CellStatus::Stale);
    assert_eq!(reopened.status("independent").unwrap(), CellStatus::Cached);
}
