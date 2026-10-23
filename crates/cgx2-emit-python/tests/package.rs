use cgx2_emit_python::{emit_package_python, write_package_python};
use cgx2_emit_rust::emit_package_rust;
use cgx2_package::load_checked_package;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-python-package-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, contents: &str) {
        let target = self.0.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, contents).unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(which: &str, scratch: &Scratch) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(which);
    for name in ["cgx2.toml", "src/main.cg", "src/math.cg"] {
        scratch.write(name, &fs::read_to_string(root.join(name)).unwrap());
    }
    if which == "package_three" {
        scratch.write(
            "src/base.cg",
            &fs::read_to_string(root.join("src/base.cg")).unwrap(),
        );
    }
    scratch.0.clone()
}

fn run_python(path: &Path, args: &[&str]) -> Output {
    Command::new("python3")
        .arg(path.join("__main__.py"))
        .args(args)
        .output()
        .unwrap()
}

fn run_native(root: &Path, binary: &Path, args: &[&str]) -> Output {
    let package = load_checked_package(root).unwrap();
    let rust = emit_package_rust(&package).unwrap();
    let source = binary.with_extension("rs");
    fs::write(&source, rust).unwrap();
    let compile = Command::new("rustc")
        .args(["--edition=2021", "-o"])
        .arg(binary)
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    Command::new(binary).args(args).output().unwrap()
}

#[test]
fn two_and_three_module_python_match_native_output_and_exit() {
    for (which, args, expected) in [
        ("package_two", vec!["a", "b"], "43\n"),
        ("package_three", vec![], "42\n"),
    ] {
        let scratch = Scratch::new();
        let root = fixture(which, &scratch);
        let package = load_checked_package(&root).unwrap();
        let files = emit_package_python(&package).unwrap();
        assert_eq!(
            files
                .keys()
                .filter(|p| p.extension().is_some_and(|e| e == "py"))
                .count(),
            if which == "package_two" { 4 } else { 5 }
        );
        if which == "package_two" {
            let math = files
                .values()
                .find(|text| text.contains("__all__ = [\"add\"]"))
                .unwrap();
            assert!(!math.contains("def hidden("));
        }
        let output = scratch.0.join("python");
        write_package_python(&package, &output).unwrap();
        for file in files.keys() {
            let syntax = Command::new("python3")
                .arg("-m")
                .arg("py_compile")
                .arg(output.join(file))
                .output()
                .unwrap();
            assert!(
                syntax.status.success(),
                "{}",
                String::from_utf8_lossy(&syntax.stderr)
            );
        }
        let python = run_python(&output, &args);
        let native = run_native(&root, &scratch.0.join("native"), &args);
        assert_eq!(python.status.success(), native.status.success());
        assert_eq!(python.stdout, native.stdout);
        assert_eq!(python.stdout, expected.as_bytes());
    }
}

#[test]
fn bool_and_string_entries_match_native_output() {
    for (ty, value, expected) in [
        ("Bool", "true", "true\n"),
        ("String", "\"hello\"", "hello\n"),
    ] {
        let scratch = Scratch::new();
        let root = fixture("package_three", &scratch);
        scratch.write(
            "src/main.cg",
            &format!("module app.main\nimport app.math\npub fn main() -> {ty}:\n    {value}\n"),
        );
        let package = load_checked_package(&root).unwrap();
        let output = scratch.0.join("python");
        write_package_python(&package, &output).unwrap();
        let python = run_python(&output, &[]);
        let native = run_native(&root, &scratch.0.join("native"), &[]);
        assert_eq!(python.status.success(), native.status.success());
        assert_eq!(python.stdout, native.stdout);
        assert_eq!(python.stdout, expected.as_bytes());
    }
}

#[test]
fn imported_nonscalar_public_signature_is_rejected_at_its_source() {
    let scratch = Scratch::new();
    let root = fixture("package_two", &scratch);
    let math = fs::read_to_string(root.join("src/math.cg")).unwrap();
    scratch.write(
        "src/math.cg",
        &format!("{math}pub fn extra(xs: List<i64>) -> i64:\n    1\n"),
    );
    let package = load_checked_package(&root).unwrap();
    let diagnostics = emit_package_python(&package).unwrap_err();
    assert!(diagnostics
        .iter()
        .any(|d| d.code == "CGE005" && d.file == Path::new("src/math.cg") && d.span.is_some()));
}

#[test]
fn no_gc_package_is_rejected_with_source_location() {
    let scratch = Scratch::new();
    let root = fixture("package_three", &scratch);
    scratch.write(
        "src/main.cg",
        "module app.main\npub fn main() -> i64:\n    no_gc(budget: 0):\n        1\n    1\n",
    );
    let errors = load_checked_package(&root).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code == "CGM001" && d.span.is_some()));
}

#[test]
fn overflow_and_contract_failure_match_native_exit_and_error_code() {
    for (body, code) in [
        ("    9223372036854775807 + 1\n", "CGR005"),
        ("    requires false\n    1\n", "CGR015"),
    ] {
        let scratch = Scratch::new();
        let root = fixture("package_three", &scratch);
        scratch.write(
            "src/main.cg",
            &format!("module app.main\nimport app.math\npub fn main() -> i64:\n{body}"),
        );
        let package = load_checked_package(&root).unwrap();
        let output = scratch.0.join("python");
        write_package_python(&package, &output).unwrap();
        let python = run_python(&output, &[]);
        let native = run_native(&root, &scratch.0.join("native"), &[]);
        assert_eq!(python.status.success(), native.status.success());
        assert!(!python.status.success());
        assert_eq!(python.stdout, native.stdout);
        assert!(String::from_utf8_lossy(&python.stderr).contains(code));
        assert!(!native.stderr.is_empty());
    }
}

#[test]
fn stale_source_cycle_and_bad_import_are_rejected() {
    let scratch = Scratch::new();
    let root = fixture("package_three", &scratch);
    let checked = load_checked_package(&root).unwrap();
    scratch.write(
        "src/base.cg",
        "module app.base\npub fn add(x: i64, y: i64) -> i64:\n    x - y\n",
    );
    assert_eq!(emit_package_python(&checked).unwrap_err()[0].code, "CGP018");

    scratch.write(
        "src/base.cg",
        "module app.base\nimport app.main\npub fn add(x: i64, y: i64) -> i64:\n    x + y\n",
    );
    assert_eq!(load_checked_package(&root).unwrap_err()[0].code, "CGP016");
    scratch.write(
        "src/base.cg",
        "module app.base\nimport app.missing\npub fn add(x: i64, y: i64) -> i64:\n    x + y\n",
    );
    assert_eq!(load_checked_package(&root).unwrap_err()[0].code, "CGP014");
}

#[cfg(unix)]
#[test]
fn manifest_symlink_substitution_is_rejected() {
    let scratch = Scratch::new();
    let root = fixture("package_two", &scratch);
    let checked = load_checked_package(&root).unwrap();
    fs::rename(root.join("cgx2.toml"), root.join("alternate.toml")).unwrap();
    std::os::unix::fs::symlink(root.join("alternate.toml"), root.join("cgx2.toml")).unwrap();
    assert_eq!(emit_package_python(&checked).unwrap_err()[0].code, "CGP018");
}

#[test]
fn traversal_existing_output_and_symlink_parent_are_rejected() {
    let scratch = Scratch::new();
    let root = fixture("package_two", &scratch);
    let package = load_checked_package(&root).unwrap();
    assert_eq!(
        write_package_python(&package, &scratch.0.join("../escape")).unwrap_err()[0].code,
        "CGE006"
    );
    let output = scratch.0.join("python");
    write_package_python(&package, &output).unwrap();
    assert_eq!(
        write_package_python(&package, &output).unwrap_err()[0].code,
        "CGE006"
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&scratch.0, scratch.0.join("link")).unwrap();
        assert_eq!(
            write_package_python(&package, &scratch.0.join("link/escape")).unwrap_err()[0].code,
            "CGE006"
        );
    }
    scratch.write("cgx2.toml", "schema_version = 1\nname = \"bad\"\nentry = \"../escape.cg\"\nsources = [\"../escape.cg\"]\n");
    assert_eq!(load_checked_package(&root).unwrap_err()[0].code, "CGP011");
}
