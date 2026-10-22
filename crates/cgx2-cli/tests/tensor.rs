use cgx2_bind::{libtorch_facade_symbols, render_lock, Binding, BindingLock};
use cgx2_cli::{check_native_package, run_native_package, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tensor-cpu");

struct Fixture {
    root: PathBuf,
    lock: BindingLock,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-tensor-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::copy(format!("{FIXTURE}/cgx2.toml"), root.join("cgx2.toml")).unwrap();
        fs::copy(format!("{FIXTURE}/src/main.cg"), root.join("src/main.cg")).unwrap();
        let mock = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../cgx2-libtorch/facade/mock.c"
        );
        assert!(Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC"])
            .arg(mock)
            .arg("-o")
            .arg(root.join("libmock.so"))
            .status()
            .unwrap()
            .success());
        let lock = BindingLock {
            schema_version: 1,
            package: "tensor_cpu_fixture".into(),
            bindings: vec![Binding {
                id: "torch_cpu".into(),
                route: "libtorch".into(),
                library: "cgx2.libtorch.cpu".into(),
                version: "2.6.0".into(),
                artifact: "libmock.so".into(),
                sha256: format!(
                    "{:x}",
                    Sha256::digest(fs::read(root.join("libmock.so")).unwrap())
                ),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                abi: "cgx2-libtorch-facade-v1".into(),
                calling_convention: "c".into(),
                layout: "tensor-f64-cpu-v1".into(),
                symbols: libtorch_facade_symbols(),
            }],
        };
        let fixture = Self { root, lock };
        fixture.write_lock();
        fixture
    }
    fn write_lock(&self) {
        fs::write(
            self.root.join("cgx2-bind.lock.json"),
            render_lock(&self.lock).unwrap(),
        )
        .unwrap();
    }
    fn source(&self) -> String {
        fs::read_to_string(self.root.join("src/main.cg")).unwrap()
    }
    fn set_source(&self, source: &str) {
        fs::write(self.root.join("src/main.cg"), source).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn checked_cg_source_runs_mock_cpu_facade_without_python() {
    let fixture = Fixture::new();
    check_native_package(&fixture.root).unwrap();
    assert_eq!(
        run_native_package(&fixture.root, &[]).unwrap().value,
        Value::String("[4.0, 6.0]".into())
    );
    let checked = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["check", fixture.root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let ran = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["run", fixture.root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(ran.stdout, b"[4.0, 6.0]\n");
    let linked = Command::new("ldd")
        .arg(env!("CARGO_BIN_EXE_cgx2-native"))
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&linked.stdout).contains("libpython"));
}

#[test]
fn source_rejections_and_artifact_errors_are_located() {
    let mut fixture = Fixture::new();
    let original = fixture.source();
    for (source, code) in [
        (original.replace("unsafe ", ""), "CGT018"),
        (original.replace("effects(Foreign)", "effects()"), "CGT018"),
        (original.replace("caps(native)", "caps()"), "CGT018"),
        (original.replace("[3.0, 4.0]", "[3.0]"), "CGT018"),
        (original.replace("[3.0, 4.0]", "[[3.0, 4.0]]"), "CGT018"),
        (original.replace("[3.0, 4.0]", "[NaN, 4.0]"), "CGT018"),
        (
            original.replace(
                "add_f64_cpu(\"[1.0, 2.0]\", \"[3.0, 4.0]\")",
                "let escaped = add_f64_cpu(\"[1.0, 2.0]\", \"[3.0, 4.0]\")\n    escaped",
            ),
            "CGT018",
        ),
    ] {
        fixture.set_source(&source);
        let errors = check_native_package(&fixture.root).unwrap_err();
        assert_eq!(errors[0].code, code, "{source}: {errors:?}");
        assert!(errors[0].span.is_some());
    }
    fixture.set_source(include_str!("fixtures/tensor-cpu/bad_shape.cg"));
    let mismatch = check_native_package(&fixture.root).unwrap_err();
    assert_eq!(mismatch[0].code, "CGT018");
    assert!(mismatch[0].message.contains("equal one-dimensional shapes"));
    let oversized = format!("[{}]", vec!["1.0"; 1025].join(", "));
    fixture.set_source(&original.replace("[3.0, 4.0]", &oversized));
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGT018"
    );
    fixture.set_source(&original);
    fixture.lock.bindings[0].sha256 = "0".repeat(64);
    fixture.write_lock();
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGB008"
    );
    fixture.lock.bindings[0].sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(fixture.root.join("libmock.so")).unwrap())
    );
    fixture.lock.bindings[0].version = "wrong".into();
    fixture.write_lock();
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGB005"
    );
    fixture.lock.bindings[0].version = "2.6.0".into();
    fixture.lock.bindings[0].artifact = "missing.so".into();
    fixture.write_lock();
    assert!(!check_native_package(&fixture.root).unwrap_err().is_empty());
}

#[test]
fn nonfinite_result_is_rejected_after_foreign_copy() {
    let fixture = Fixture::new();
    fixture.set_source(
        &fixture
            .source()
            .replace("[1.0, 2.0]", "[1e308, 1e308]")
            .replace("[3.0, 4.0]", "[1e308, 1e308]"),
    );
    check_native_package(&fixture.root).unwrap();
    let errors = run_native_package(&fixture.root, &[]).unwrap_err();
    assert_eq!(errors[0].code, "CGR024");
    assert!(errors[0].span.is_some());
}
