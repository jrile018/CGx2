use cgx2_emit_python::{emit_package_python, write_package_python};
use cgx2_emit_rust::emit_package_rust;
use cgx2_package::load_checked_package;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_SCRATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-python-package-{}-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
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

fn run_python_limited(path: &Path) -> Output {
    Command::new("timeout")
        .args(["15s", "python3"])
        .arg(path.join("__main__.py"))
        .output()
        .unwrap()
}

#[test]
fn range_loops_preserve_scopes_calls_and_integer_bounds() {
    let scratch = Scratch::new();
    let root = fixture("package_three", &scratch);
    scratch.write(
        "src/math.cg",
        "module app.math\npub fn add_one(x: i64) -> i64:\n    x + 1\n",
    );
    scratch.write("src/main.cg", "module app.main\nimport app.math\nfn bump(x: i64) -> i64:\n    x + 1\npub fn main() -> i64:\n    let mut total = 0\n    let i = 99\n    for i in range(0, 3):\n        for i in range(i, i + 2):\n            total += bump(i)\n    for zero in range(0, 0):\n        total += 100\n    for negative in range(-3, 0):\n        total += negative\n    for edge in range(9223372036854775806, 9223372036854775807):\n        total += 1\n    for edge in range(-9223372036854775808, -9223372036854775807):\n        total += 1\n    total + i + app.math.add_one(-1)\n");
    let package = load_checked_package(&root).unwrap();
    let files = emit_package_python(&package).unwrap();
    assert_eq!(files, emit_package_python(&package).unwrap());
    assert!(files
        .values()
        .any(|source| source.contains("# CGx2 bytes [")));
    let output = scratch.0.join("python");
    write_package_python(&package, &output).unwrap();
    let python = run_python_limited(&output);
    let native = run_native(&root, &scratch.0.join("native"), &[]);
    assert_eq!(
        python.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&python.stderr)
    );
    assert_eq!(python.stdout, native.stdout);
    assert_eq!(python.stdout, b"110\n");
}

#[test]
fn range_loop_overflow_and_budget_fail_explicitly() {
    for (body, code) in [
        ("    let mut total = 9223372036854775807\n    for i in range(0, 1):\n        total += 1\n    total\n", "CGR005"),
        ("    for i in range(0, 1000000):\n        1\n    0\n", "CGR010"),
    ] {
        let scratch = Scratch::new();
        let root = fixture("package_three", &scratch);
        scratch.write("src/main.cg", &format!("module app.main\npub fn main() -> i64:\n{body}"));
        let package = load_checked_package(&root).unwrap();
        let output = scratch.0.join("python");
        write_package_python(&package, &output).unwrap();
        let python = run_python_limited(&output);
        let native = run_native(&root, &scratch.0.join("native"), &[]);
        assert_ne!(python.status.code(), Some(124), "Python subprocess exceeded 15 seconds");
        assert!(!python.status.success());
        assert_eq!(python.status.success(), native.status.success());
        assert!(String::from_utf8_lossy(&python.stderr).contains(code));
        assert!(String::from_utf8_lossy(&native.stderr).contains(code));
    }
}

#[test]
fn package_call_depth_matches_native_limit() {
    let scratch = Scratch::new();
    let root = fixture("package_three", &scratch);
    let mut source = String::from("module app.main\npub fn main() -> i64:\n    f0()\n");
    for index in 0..65 {
        source.push_str(&format!("fn f{index}() -> i64:\n"));
        if index == 64 {
            source.push_str("    1\n");
        } else {
            source.push_str(&format!("    f{}()\n", index + 1));
        }
    }
    scratch.write("src/main.cg", &source);
    let package = load_checked_package(&root).unwrap();
    let output = scratch.0.join("python");
    write_package_python(&package, &output).unwrap();
    let python = run_python_limited(&output);
    let native = run_native(&root, &scratch.0.join("native"), &[]);
    assert_ne!(
        python.status.code(),
        Some(124),
        "Python subprocess exceeded 15 seconds"
    );
    assert!(!python.status.success());
    assert_eq!(python.status.success(), native.status.success());
    assert!(String::from_utf8_lossy(&python.stderr).contains("CGR011"));
    assert!(String::from_utf8_lossy(&native.stderr)
        .contains("function call depth limit of 64 exceeded"));
}

#[test]
fn range_loop_step_boundary_matches_native() {
    // Six fixed steps (range statement, iterator, two bounds, final expression)
    // plus three steps per iteration (iteration, body statement, body expression).
    for (end, succeeds) in [(333_331, true), (333_332, false)] {
        let scratch = Scratch::new();
        let root = fixture("package_three", &scratch);
        scratch.write("src/main.cg", &format!("module app.main\npub fn main() -> i64:\n    for i in range(0, {end}):\n        1\n    0\n"));
        let package = load_checked_package(&root).unwrap();
        let output = scratch.0.join("python");
        write_package_python(&package, &output).unwrap();
        let python = run_python_limited(&output);
        let native = run_native(&root, &scratch.0.join("native"), &[]);
        assert_ne!(
            python.status.code(),
            Some(124),
            "Python subprocess exceeded 15 seconds"
        );
        assert_eq!(python.status.success(), succeeds);
        assert_eq!(python.status.success(), native.status.success());
        if succeeds {
            assert_eq!(python.stdout, b"0\n");
            assert_eq!(python.stdout, native.stdout);
        } else {
            assert!(String::from_utf8_lossy(&python.stderr).contains("CGR010"));
        }
    }
}

#[test]
fn malformed_range_bounds_keep_source_locations() {
    for (iterator, code) in [("range(1)", "CGT005"), ("range(true, 2)", "CGT003")] {
        let scratch = Scratch::new();
        let root = fixture("package_three", &scratch);
        scratch.write("src/main.cg", &format!("module app.main\npub fn main() -> i64:\n    for i in {iterator}:\n        1\n    0\n"));
        let diagnostics = load_checked_package(&root).unwrap_err();
        assert!(diagnostics
            .iter()
            .any(|d| d.code == code && d.file.ends_with("src/main.cg") && d.span.is_some()));
    }
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
fn imported_collection_public_signature_is_admitted() {
    let scratch = Scratch::new();
    let root = fixture("package_two", &scratch);
    let math = fs::read_to_string(root.join("src/math.cg")).unwrap();
    scratch.write(
        "src/math.cg",
        &format!("{math}pub fn extra(xs: List<i64>) -> i64:\n    1\n"),
    );
    let package = load_checked_package(&root).unwrap();
    let output = scratch.0.join("python-values");
    write_package_python(&package, &output).unwrap();
    let result = run_python(&output, &[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout, b"41\n");
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
    // Ordinary no_gc now selects the authenticated owned profile. Packages
    // still reject that profile before the copied-value/package walker.
    assert!(errors
        .iter()
        .any(|d| d.code == "CGT080" && d.span.is_some()));
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

#[test]
fn conditional_package_imports_lower_guard_and_both_scoped_branches() {
    let scratch = Scratch::new();
    scratch.write("cgx2.toml", "schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\", \"src/choice.cg\", \"src/base.cg\"]\n");
    scratch.write("src/main.cg", "module demo.main\nimport demo.choice\npub fn main() -> i64:\n    demo.choice.choose(false)\n");
    scratch.write("src/choice.cg", "module demo.choice\nimport demo.base\npub fn choose(flag: Bool) -> i64:\n    if demo.base.guard(flag):\n        let result = demo.base.answer() / 0\n        return result\n    else:\n        let result = demo.base.answer()\n        return result\n");
    scratch.write("src/base.cg", "module demo.base\npub fn guard(flag: Bool) -> Bool:\n    flag\npub fn answer() -> i64:\n    42\n");
    let package = load_checked_package(&scratch.0).unwrap();
    let directory = scratch.0.join("conditional-python");
    write_package_python(&package, &directory).unwrap();
    let output = run_python(&directory, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"42\n");
    assert!(emit_package_rust(&package).is_ok());
}
