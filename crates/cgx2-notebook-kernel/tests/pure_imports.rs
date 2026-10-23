use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use cgx2_notebook_format::{parse_notebook, write_notebook, Cell, CellKind, Notebook, Output};
use cgx2_notebook_kernel::NotebookKernel;
use cgx2_notebook_session::CellStatus;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-notebook-pure-imports-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let project = Self(root);
        project.write("cgx2.toml", "schema_version = 1\nname = \"demo\"\nentry = \"src/alpha.cg\"\nsources = [\"src/alpha.cg\", \"src/beta.cg\"]\n");
        project.write(
            "src/alpha.cg",
            "module demo.alpha\npub fn double(x: i64) -> i64 effects() caps():\n x * 2\n",
        );
        project.write(
            "src/beta.cg",
            "module demo.beta\npub fn triple(x: i64) -> i64 effects() caps():\n x * 3\n",
        );
        project
    }

    fn root(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
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

fn notebook() -> Notebook {
    Notebook {
        version: 1,
        cells: vec![
            code("alpha_import", "import demo.alpha\n"),
            code("alpha_value", "let value = alpha.double(21)\n"),
            code("beta_import", "import demo.beta\n"),
            code("alpha_result", "value + 1\n"),
            code("beta_result", "beta.triple(7)\n"),
        ],
    }
}

fn body(cell: &Cell) -> &str {
    match &cell.outputs[0] {
        Output::Text { body, .. } => body,
        _ => panic!("expected text"),
    }
}

#[test]
fn two_modules_selective_rerun_and_saved_provenance() {
    let project = Project::new();
    let mut kernel = NotebookKernel::open_with_project(notebook(), Some(project.root())).unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(body(&kernel.notebook().cells[3]), "43");
    assert_eq!(body(&kernel.notebook().cells[4]), "21");
    let beta_revision = kernel.notebook().cells[4]
        .provenance
        .as_ref()
        .unwrap()
        .ordinal;
    let alpha_package = kernel.notebook().cells[1]
        .provenance
        .as_ref()
        .unwrap()
        .package
        .as_ref()
        .unwrap();
    assert_eq!(alpha_package.modules.len(), 1);
    assert!(alpha_package.modules.contains_key("demo.alpha"));
    assert_eq!(alpha_package.manifest_sha256.len(), 64);

    kernel
        .replace_cell("alpha_value", "let value = alpha.double(22)\n".into())
        .unwrap();
    kernel.rerun_affected("alpha_value").unwrap();
    assert_eq!(body(&kernel.notebook().cells[3]), "45");
    assert_eq!(
        kernel.notebook().cells[4]
            .provenance
            .as_ref()
            .unwrap()
            .ordinal,
        beta_revision
    );
    let saved = write_notebook(kernel.notebook()).unwrap();
    assert!(saved.contains("module_sha256=demo.alpha:"));
    assert!(saved.contains("manifest_sha256="));
    let reopened =
        NotebookKernel::open_with_project(parse_notebook(&saved).unwrap(), Some(project.root()))
            .unwrap();
    assert_eq!(reopened.status("alpha_result").unwrap(), CellStatus::Cached);
    assert_eq!(reopened.status("beta_result").unwrap(), CellStatus::Cached);
    let mut clean = reopened;
    clean.restart_and_run_all().unwrap();
    assert_eq!(body(&clean.notebook().cells[3]), "45");
    assert_eq!(body(&clean.notebook().cells[4]), "21");
}

#[test]
fn changed_source_stales_only_dependents_and_missing_project_fails() {
    let project = Project::new();
    let mut kernel = NotebookKernel::open_with_project(notebook(), Some(project.root())).unwrap();
    kernel.restart_and_run_all().unwrap();
    let saved = write_notebook(kernel.notebook()).unwrap();
    assert!(NotebookKernel::open(parse_notebook(&saved).unwrap())
        .unwrap_err()
        .to_string()
        .contains("--project"));
    project.write(
        "src/alpha.cg",
        "module demo.alpha\npub fn double(x: i64) -> i64 effects() caps():\n x * 4\n",
    );
    let reopened =
        NotebookKernel::open_with_project(parse_notebook(&saved).unwrap(), Some(project.root()))
            .unwrap();
    for id in ["alpha_import", "alpha_value", "alpha_result"] {
        assert_eq!(reopened.status(id).unwrap(), CellStatus::Stale, "{id}");
    }
    assert_eq!(reopened.status("beta_result").unwrap(), CellStatus::Cached);
    assert!(
        reopened.notebook().cells[3]
            .provenance
            .as_ref()
            .unwrap()
            .stale
    );
}

#[test]
fn bool_parameter_and_return_execute_through_scalar_interpreter() {
    let project = Project::new();
    project.write(
        "src/beta.cg",
        "module demo.beta\npub fn same(flag: Bool) -> Bool effects() caps():\n flag\n",
    );
    let notebook = Notebook {
        version: 1,
        cells: vec![
            code("import", "import demo.beta\n"),
            code("call", "beta.same(true)\n"),
        ],
    };
    let mut kernel = NotebookKernel::open_with_project(notebook, Some(project.root())).unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(body(&kernel.notebook().cells[1]), "true");
}

#[test]
fn notebook_pure_function_can_call_directly_imported_module() {
    let project = Project::new();
    let notebook = Notebook {
        version: 1,
        cells: vec![
            code("import", "import demo.alpha\n"),
            code(
                "define",
                "fn apply(x: i64) -> i64 effects() caps():\n alpha.double(x)\n",
            ),
            code("call", "apply(5)\n"),
        ],
    };
    let mut kernel = NotebookKernel::open_with_project(notebook, Some(project.root())).unwrap();
    kernel.restart_and_run_all().unwrap();
    assert_eq!(body(&kernel.notebook().cells[2]), "10");
}

#[test]
fn rejects_nonpublic_effectful_duplicate_and_transitive_calls() {
    let project = Project::new();
    let hidden = Notebook {
        version: 1,
        cells: vec![
            code("import", "import demo.alpha\n"),
            code("call", "alpha.hidden(2)\n"),
        ],
    };
    let error = NotebookKernel::open_with_project(hidden, Some(project.root()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("alpha.hidden") && error.contains("at "),
        "{error}"
    );

    project.write(
        "src/alpha.cg",
        "module demo.alpha\npub fn double(x: i64) -> i64 effects(ManagedAlloc) caps():\n x * 2\n",
    );
    let error = NotebookKernel::open_with_project(notebook(), Some(project.root()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("demo.alpha") && error.contains("effects()"),
        "{error}"
    );

    project.write("src/alpha.cg", "module demo.alpha\nimport demo.beta\npub fn double(x: i64) -> i64 effects() caps():\n demo.beta.triple(x)\n");
    let error = NotebookKernel::open_with_project(notebook(), Some(project.root()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("CGT012") && error.contains("alpha.cg"),
        "{error}"
    );

    project.write(
        "src/alpha.cg",
        "module demo.alpha\npub fn double(x: i64) -> i64 effects() caps():\n x * 2\n",
    );

    let duplicate = Notebook {
        version: 1,
        cells: vec![
            code("one", "import demo.alpha\n"),
            code("two", "import demo.alpha\n"),
        ],
    };
    let error = NotebookKernel::open_with_project(duplicate, Some(project.root()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("duplicate module short name alpha"),
        "{error}"
    );
}

#[test]
fn malformed_manifest_cycle_and_mutation_after_open_fail_closed() {
    let project = Project::new();
    project.write("cgx2.toml", "schema_version = 1\nname = \"demo\"\nentry = \"../secret.cg\"\nsources = [\"../secret.cg\"]\n");
    assert!(
        NotebookKernel::open_with_project(notebook(), Some(project.root()))
            .unwrap_err()
            .to_string()
            .contains("CGP")
    );
    project.write("cgx2.toml", "schema_version = 1\nname = \"demo\"\nentry = \"src/alpha.cg\"\nsources = [\"src/alpha.cg\", \"src/beta.cg\"]\n");
    project.write("src/alpha.cg", "module demo.alpha\nimport demo.beta\npub fn double(x: i64) -> i64 effects() caps():\n demo.beta.triple(x)\n");
    project.write("src/beta.cg", "module demo.beta\nimport demo.alpha\npub fn triple(x: i64) -> i64 effects() caps():\n demo.alpha.double(x)\n");
    assert!(
        NotebookKernel::open_with_project(notebook(), Some(project.root()))
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    project.write(
        "src/alpha.cg",
        "module demo.alpha\npub fn double(x: i64) -> i64 effects() caps():\n x * 2\n",
    );
    project.write(
        "src/beta.cg",
        "module demo.beta\npub fn triple(x: i64) -> i64 effects() caps():\n x * 3\n",
    );
    let mut kernel = NotebookKernel::open_with_project(notebook(), Some(project.root())).unwrap();
    project.write(
        "src/alpha.cg",
        "module demo.alpha\npub fn double(x: i64) -> i64 effects() caps():\n x * 5\n",
    );
    assert!(kernel
        .run_cell("alpha_import")
        .unwrap_err()
        .to_string()
        .contains("CGP018"));
}
