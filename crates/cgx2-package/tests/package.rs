use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use cgx2_package::{load_checked_package, load_package};

#[test]
fn checked_snapshot_rejects_manifest_and_source_mutations() {
    let f = Fixture::new();
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    f.write(
        "src/main.cg",
        "module demo.main\nfn main() -> i64:\n    7\n",
    );
    let checked = load_checked_package(f.root()).unwrap();
    assert!(checked.verify_unchanged().is_ok());
    assert_eq!(checked.modules()[0].source(), Path::new("src/main.cg"));
    f.write(
        "src/main.cg",
        "module demo.main\nfn main() -> i64:\n    8\n",
    );
    assert_eq!(checked.verify_unchanged().unwrap_err()[0].code, "CGP018");
    let checked = load_checked_package(f.root()).unwrap();
    f.manifest("schema_version = 1\nname = \"new\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    assert_eq!(checked.verify_unchanged().unwrap_err()[0].code, "CGP018");
}

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-package-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn root(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, content: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn manifest(&self, body: &str) {
        self.write("cgx2.toml", body);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str = "schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\", \"src/math.cg\"]\n";

fn codes(fixture: &Fixture) -> Vec<&'static str> {
    load_package(fixture.root())
        .unwrap_err()
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn two_modules_have_sorted_public_api_and_stable_rendering() {
    let f = Fixture::new();
    f.manifest(MANIFEST);
    f.write(
        "src/main.cg",
        "module demo.main\npub fn run(x: i64) -> i64:\n    x\n",
    );
    f.write(
        "src/math.cg",
        "module demo.math\nfn hidden(x: i64) -> i64:\n    x\npub fn twice(x: i64) -> i64:\n    x * 2\n",
    );
    let first = load_package(f.root()).unwrap();
    let second = load_package(f.root()).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.entry, PathBuf::from("src/main.cg"));
    assert_eq!(first.modules.len(), 2);
    assert_eq!(first.modules[0].name.as_deref(), Some("demo.main"));
    assert_eq!(first.modules[1].name.as_deref(), Some("demo.math"));
    assert_eq!(first.modules[0].api.functions[0].name, "run");
    assert_eq!(first.modules[1].api.functions[0].name, "twice");
    assert_eq!(first.modules[1].api.functions.len(), 1);
    assert!(first.imports_supported);
    assert_eq!(first.render_text(), second.render_text());
    assert_eq!(first.render_json(), second.render_json());
    assert!(first.render_json().contains("\"imports_supported\":true"));
    assert!(first.render_json().contains("\"return_type\":\"i64\""));
}

#[test]
fn committed_example_matches_expected_index() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo");
    let index = load_package(&root).unwrap();
    let expected = include_str!("../fixtures/demo/expected-index.json").trim_end();
    assert_eq!(index.render_json(), expected);
    assert_eq!(index.modules[0].name.as_deref(), Some("demo.main"));
    assert_eq!(index.modules[1].name.as_deref(), Some("demo.math"));
}

#[test]
fn manifest_rejects_missing_duplicate_unknown_and_malformed_values() {
    let f = Fixture::new();
    f.manifest("name = \"demo\"\n");
    let errors = load_package(f.root()).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code == "CGP006" && d.span.is_some()));
    f.manifest("schema_version = 1\nname = \"demo\"\nname = \"again\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP002"));
    f.manifest("schema_version = 1\nname = \"demo\"\nextra = 1\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP001"));
    f.manifest("schema_version = 1\nname = \"unterminated\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP003"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\", \"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP007"));
}

#[test]
fn invalid_utf8_and_missing_entry_are_reported() {
    let f = Fixture::new();
    fs::write(f.root().join("cgx2.toml"), [0xff]).unwrap();
    assert!(codes(&f).contains(&"CGP004"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/other.cg\"\nsources = [\"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP008"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    fs::create_dir_all(f.root().join("src")).unwrap();
    fs::write(f.root().join("src/main.cg"), [0xff]).unwrap();
    let errors = load_package(f.root()).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code == "CGP004" && d.file.ends_with("src/main.cg")));
}

#[test]
fn size_limits_and_missing_file_are_explicit() {
    let f = Fixture::new();
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    let missing = load_package(f.root()).unwrap_err();
    assert!(missing.iter().any(|d| d.code == "CGP005"
        && d.file.ends_with("src/main.cg")
        && d.message.contains("src/main.cg")));
    fs::create_dir_all(f.root().join("src")).unwrap();
    fs::write(
        f.root().join("src/main.cg"),
        vec![b' '; cgx2_package::MAX_SOURCE_BYTES + 1],
    )
    .unwrap();
    assert!(codes(&f).contains(&"CGP013"));
    fs::write(
        f.root().join("cgx2.toml"),
        vec![b' '; cgx2_package::MAX_MANIFEST_BYTES + 1],
    )
    .unwrap();
    assert!(codes(&f).contains(&"CGP013"));
}

#[test]
fn source_errors_and_duplicate_module_names_keep_source_locations() {
    let f = Fixture::new();
    f.manifest(MANIFEST);
    f.write(
        "src/main.cg",
        "module demo.same\npub fn run(x: i64) -> i64:\n    x\n",
    );
    f.write(
        "src/math.cg",
        "module demo.same\npub fn twice(x: i64) -> i64:\n    x\n",
    );
    assert!(codes(&f).contains(&"CGP010"));
    f.write(
        "src/math.cg",
        "module demo.math\npub fn twice(x: i64) -> i64:\n    nope\n",
    );
    let errors = load_package(f.root()).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code.starts_with("CGT") && d.span.is_some() && d.file.ends_with("src/math.cg")));
    f.write(
        "src/math.cg",
        "module demo.math\npub fn twice(x: i64):\n    x\n",
    );
    assert!(codes(&f).iter().any(|code| code.starts_with("CGT")));
    f.write("src/math.cg", "module demo.math\npub fn twice(:\n");
    assert!(codes(&f).iter().any(|code| code.starts_with("CGS")));
    f.write("src/math.cg", "pub fn twice(x: i64) -> i64:\n    x\n");
    assert!(codes(&f).contains(&"CGP009"));
}

#[test]
fn paths_cannot_escape_and_aliases_cannot_duplicate_sources() {
    let f = Fixture::new();
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"../outside.cg\"\nsources = [\"../outside.cg\"]\n");
    assert!(codes(&f).contains(&"CGP011"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/./main.cg\"\nsources = [\"src/./main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP011"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src//main.cg\"\nsources = [\"src//main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP011"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\", \"src/alias.cg\"]\n");
    f.write("src/main.cg", "module demo.main\n");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("main.cg", f.root().join("src/alias.cg")).unwrap();
        assert!(codes(&f).contains(&"CGP007"));
    }
}

#[cfg(unix)]
#[test]
fn outside_symlink_is_rejected_before_source_read() {
    let f = Fixture::new();
    let outside = Fixture::new();
    outside.write("secret.cg", "module secret\n");
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    fs::create_dir_all(f.root().join("src")).unwrap();
    std::os::unix::fs::symlink(
        outside.root().join("secret.cg"),
        f.root().join("src/main.cg"),
    )
    .unwrap();
    let errors = load_package(f.root()).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code == "CGP011" && d.file.ends_with("src/main.cg")));
}

#[test]
fn three_module_fixture_checks_calls_and_dependency_order() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/imports");
    let index = load_package(&root).unwrap();
    assert_eq!(index.schema_version, 2);
    assert_eq!(
        index.topological_order,
        ["app.base", "app.math", "app.main"]
    );
    let main = index
        .modules
        .iter()
        .find(|m| m.name.as_deref() == Some("app.main"))
        .unwrap();
    assert_eq!(main.imports, ["app.math"]);
    assert_eq!(main.resolved_public_calls[0].name, "app.math.double");
    let source = fs::read_to_string(root.join(&main.source)).unwrap();
    let span = main.resolved_public_calls[0].span;
    assert_eq!(&source[span.start..span.end], "app.math.double(3)");
    assert_eq!(
        index.render_json(),
        load_package(&root).unwrap().render_json()
    );
}

#[test]
fn two_module_public_call_checks_from_manifest_entry() {
    let f = Fixture::new();
    f.manifest(MANIFEST);
    f.write(
        "src/main.cg",
        "module app.main\nimport app.math\npub fn run() -> i64:\n    app.math.double(3)\n",
    );
    f.write(
        "src/math.cg",
        "module app.math\npub fn double(x: i64) -> i64:\n    x * 2\n",
    );
    let index = load_package(f.root()).unwrap();
    assert_eq!(index.topological_order, ["app.math", "app.main"]);
    assert_eq!(
        index
            .modules
            .iter()
            .find(|m| m.name.as_deref() == Some("app.main"))
            .unwrap()
            .resolved_public_calls[0]
            .name,
        "app.math.double"
    );
}

#[test]
fn import_failures_are_distinct_and_source_located() {
    let f = Fixture::new();
    f.manifest(MANIFEST);
    f.write("src/math.cg", "module app.math\nfn hidden(x: i64) -> i64:\n    x\npub fn double(x: i64) -> i64:\n    x * 2\n");
    let check = |source: &str| {
        f.write("src/main.cg", source);
        load_package(f.root()).unwrap_err()
    };
    let missing_module = check("module app.main\nimport app.absent\npub fn run() -> i64:\n    1\n");
    assert!(missing_module.iter().any(|d| d.code == "CGP014"
        && d.span == Some(cgx2_syntax::Span { start: 23, end: 33 })
        && d.file.ends_with("src/main.cg")));
    let private =
        check("module app.main\nimport app.math\npub fn run() -> i64:\n    app.math.hidden(1)\n");
    assert!(private
        .iter()
        .any(|d| d.code == "CGT009" && d.file.ends_with("src/main.cg") && d.span.is_some()));
    let unimported = check("module app.main\npub fn run() -> i64:\n    app.math.double(1)\n");
    assert!(unimported.iter().any(|d| d.code == "CGT008"));
    let missing_symbol =
        check("module app.main\nimport app.math\npub fn run() -> i64:\n    app.math.absent(1)\n");
    assert!(missing_symbol.iter().any(|d| d.code == "CGT001"));
    let wrong_type = check(
        "module app.main\nimport app.math\npub fn run() -> i64:\n    app.math.double(true)\n",
    );
    assert!(wrong_type.iter().any(|d| d.code == "CGT003"));
    assert!(wrong_type.iter().any(|d| d.code == "CGT003"
        && d.related
            .as_ref()
            .is_some_and(|r| r.file.ends_with("src/math.cg"))));
    let arity =
        check("module app.main\nimport app.math\npub fn run() -> i64:\n    app.math.double()\n");
    assert!(arity.iter().any(|d| d.code == "CGT005"));
    let self_import = check("module app.main\nimport app.main\npub fn run() -> i64:\n    1\n");
    assert!(self_import.iter().any(|d| d.code == "CGP015"));
}

#[test]
fn cycle_trace_is_stable_and_points_at_closing_import() {
    let f = Fixture::new();
    f.manifest(MANIFEST);
    f.write(
        "src/main.cg",
        "module app.main\nimport app.math\npub fn run() -> i64:\n    1\n",
    );
    f.write(
        "src/math.cg",
        "module app.math\nimport app.main\npub fn double(x: i64) -> i64:\n    x\n",
    );
    let first = load_package(f.root()).unwrap_err();
    let second = load_package(f.root()).unwrap_err();
    assert_eq!(first, second);
    let cycle = first.iter().find(|d| d.code == "CGP016").unwrap();
    assert_eq!(
        cycle.message,
        "import cycle: app.main -> app.math -> app.main"
    );
    assert!(cycle.file.ends_with("src/math.cg"));
    assert_eq!(cycle.span, Some(cgx2_syntax::Span { start: 23, end: 31 }));
}

#[test]
fn importing_memory_script_is_explicitly_unsupported() {
    let f = Fixture::new();
    f.manifest(MANIFEST);
    f.write(
        "src/main.cg",
        "module app.main\nimport app.math\npub fn run() -> i64:\n    1\n",
    );
    f.write(
        "src/math.cg",
        "module app.math\nlet region = memory.region(mode: manual, limit: 32)\n",
    );
    assert!(codes(&f).contains(&"CGP017"));
}

#[test]
fn check_command_emits_json_and_fails_on_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/imports");
    let binary = env!("CARGO_BIN_EXE_cgx2-package");
    let output = std::process::Command::new(binary)
        .args(["check", root.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("\"topological_order\":[\"app.base\",\"app.math\",\"app.main\"]"));
    let f = Fixture::new();
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    f.write("src/main.cg", "module app.main\nimport app.absent\n");
    let output = std::process::Command::new(binary)
        .args(["check", f.root().to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("\"code\":\"CGP014\""));

    f.manifest(MANIFEST);
    f.write(
        "src/main.cg",
        "module app.main\nimport app.math\npub fn run() -> i64:\n    app.math.double(true)\n",
    );
    f.write(
        "src/math.cg",
        "module app.math\npub fn double(x: i64) -> i64:\n    x\n",
    );
    let output = std::process::Command::new(binary)
        .args(["check", f.root().to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let line = String::from_utf8(output.stdout).unwrap();
    assert!(line.contains("\"code\":\"CGT003\""));
    assert!(line.contains("\"related\":{\"file\":\"src/math.cg\""));
}

#[test]
fn imported_list_signature_is_outside_scalar_profile() {
    let f = Fixture::new();
    f.manifest(MANIFEST);
    f.write("src/main.cg", "module app.main\nimport app.math\npub fn run(xs: List<i64>) -> i64:\n    app.math.size(xs)\n");
    f.write(
        "src/math.cg",
        "module app.math\npub fn size(xs: List<i64>) -> i64:\n    1\n",
    );
    assert!(codes(&f).contains(&"CGT010"));
}

#[test]
fn public_records_keep_defining_identity_and_visibility() {
    let f = Fixture::new();
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/right.cg\", \"src/main.cg\", \"src/left.cg\"]\n");
    f.write("src/left.cg", "module demo.left\npub record Point:\n    x: i64\npub fn identity(p: Point) -> Point:\n    p\n");
    f.write("src/right.cg", "module demo.right\nimport demo.left\npub record Point:\n    x: i64\npub fn pass(p: demo.left.Point) -> demo.left.Point:\n    demo.left.identity(p)\n");
    let good = "module demo.main\nimport demo.left\nimport demo.right\nfn main() -> i64:\n    let p = demo.right.pass(demo.left.Point(x: 4))\n    p.x\n";
    f.write("src/main.cg", good);
    let checked = cgx2_package::load_checked_package(f.root()).unwrap();
    let right = checked
        .modules()
        .iter()
        .find(|m| m.source() == Path::new("src/right.cg"))
        .unwrap();
    assert_eq!(
        right.typed().functions["pass"].return_type.to_string(),
        "demo.left.Point"
    );
    f.write(
        "src/main.cg",
        &good.replace("demo.left.Point(x: 4)", "demo.right.Point(x: 4)"),
    );
    assert!(codes(&f).contains(&"CGT003"));
    f.write("src/main.cg", &good.replace("import demo.left\n", ""));
    assert!(codes(&f).contains(&"CGT008"));
    f.write("src/main.cg", good);
    f.write(
        "src/left.cg",
        "module demo.left\nrecord Point:\n    x: i64\npub fn identity(p: Point) -> Point:\n    p\n",
    );
    assert!(codes(&f).contains(&"CGT022"));
    f.write(
        "src/left.cg",
        "module demo.left\nrecord Point:\n    x: i64\n",
    );
    f.write("src/right.cg", "module demo.right\n");
    f.write("src/main.cg", "module demo.main\nimport demo.left\nfn main() -> i64:\n    let p = demo.left.Point(x: 4)\n    p.x\n");
    assert!(codes(&f).contains(&"CGT022"));
    f.write("src/main.cg", "module demo.main\nimport demo.left\nfn consume(p: demo.left.Point) -> i64:\n    p.x\nfn main() -> i64:\n    0\n");
    assert!(codes(&f).contains(&"CGT022"));
}

#[test]
fn public_imported_record_construction_requires_direct_defining_imports() {
    let f = Fixture::new();
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/right.cg\", \"src/main.cg\", \"src/left.cg\"]\n");
    f.write(
        "src/left.cg",
        "module demo.left\npub record Point:\n    x: i64\n",
    );
    f.write("src/right.cg", "module demo.right\nimport demo.left\npub fn make() -> demo.left.Point effects() caps():\n    demo.left.Point(x: 7)\n");
    let main = "module demo.main\nimport demo.right\nfn main() -> i64:\n    let p = demo.right.make()\n    p.x\n";
    f.write("src/main.cg", main);
    assert!(codes(&f).contains(&"CGT008"));
    f.write(
        "src/main.cg",
        &main.replace("import demo.right", "import demo.left\nimport demo.right"),
    );
    cgx2_package::load_checked_package(f.root()).unwrap();
    f.write("src/right.cg", "module demo.right\nimport demo.left\npub record Point:\n    x: i64\npub fn wrong() -> demo.left.Point:\n    Point(x: 7)\n");
    assert!(codes(&f).contains(&"CGT003"));
    f.write("src/right.cg", "module demo.right\nimport demo.left\npub fn bad() -> demo.left.Missing:\n    demo.left.Point(x: 7)\n");
    assert!(codes(&f).contains(&"CGT002"));
}
