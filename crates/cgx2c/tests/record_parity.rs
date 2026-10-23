use cgx2_cli::run_checked_source;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-record-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name)
}

#[test]
fn package_debug_release_matches_source_result_and_error_class() {
    for (package, expected, error_class) in [
        ("record_package", "-9223372036854775808", None),
        ("record_error_package", "", Some("CGR006")),
    ] {
        let root = fixture(package);
        let source = fs::read_to_string(root.join("src/main.cg")).unwrap();
        let emitted = TempDir::new();
        let rust = emitted.path().join("program.rs");
        let emitted_result = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
            .args(["emit-rust", "--package"])
            .arg(&root)
            .args(["--out"])
            .arg(&rust)
            .output()
            .unwrap();
        assert!(
            emitted_result.status.success(),
            "{}",
            String::from_utf8_lossy(&emitted_result.stderr)
        );
        let generated = fs::read_to_string(&rust).unwrap();
        assert!(generated.contains("#[derive(Clone, Copy)]"));
        assert!(generated.contains("struct cgx2_user_"));
        let source_result = run_checked_source(&source, &[]);
        if let Some(code) = error_class {
            assert_eq!(source_result.unwrap_err()[0].code, code);
        } else {
            assert_eq!(source_result.unwrap().0.value.to_string(), expected);
        }
        for release in [false, true] {
            let dir = TempDir::new();
            let binary = dir.path().join("record-program");
            let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
            command
                .args(["build", "--package"])
                .arg(&root)
                .args(["--out"])
                .arg(&binary);
            if release {
                command.arg("--release");
            }
            let built = command.output().unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            let result = Command::new(&binary).output().unwrap();
            if let Some(code) = error_class {
                assert!(!result.status.success());
                assert!(String::from_utf8_lossy(&result.stderr).contains(code));
            } else {
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), expected);
            }
        }
    }
}

#[test]
fn public_package_records_build_in_both_profiles_and_fail_atomically() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../cgx2-package/fixtures/public-records");
    for release in [false, true] {
        let dir = TempDir::new();
        let binary = dir.path().join("public-record-program");
        let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
        command
            .args(["build", "--package"])
            .arg(&root)
            .arg("--out")
            .arg(&binary);
        if release {
            command.arg("--release");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(&binary).output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            output.stdout,
            b"demo.geometry.Point(self: 42, live: true)\n"
        );
        let output = Command::new(&binary).arg("fail").output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("CGR005"));
    }
    let source = TempDir::new();
    fs::create_dir(source.path().join("src")).unwrap();
    for file in ["cgx2.toml", "src/main.cg", "src/geometry.cg"] {
        let text = fs::read_to_string(root.join(file))
            .unwrap()
            .replace("pub record Point:", "record Point:");
        fs::write(source.path().join(file), text).unwrap();
    }
    let output = TempDir::new();
    let destination = output.path().join("previous.rs");
    fs::write(&destination, "previous output\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
        .args(["emit-rust", "--package"])
        .arg(source.path())
        .arg("--out")
        .arg(&destination)
        .arg("--force")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("CGT022"));
    assert_eq!(
        fs::read_to_string(destination).unwrap(),
        "previous output\n"
    );
}
