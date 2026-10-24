use cgx2_cli::run_native_package;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-s2a-records-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../cgx2-emit-rust/tests/fixtures")
        .join(name)
}

fn copy_fixture(name: &str, temp: &Temp) {
    fs::create_dir(temp.0.join("src")).unwrap();
    fs::copy(fixture(name).join("cgx2.toml"), temp.0.join("cgx2.toml")).unwrap();
    for entry in fs::read_dir(fixture(name).join("src")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), temp.0.join("src").join(entry.file_name())).unwrap();
    }
}

fn parity(root: &Path, expected: &str) {
    assert_eq!(
        run_native_package(root, &[]).unwrap().value.to_string(),
        expected
    );
    let temp = Temp::new();
    for release in [false, true] {
        let binary = temp.0.join(if release { "release" } else { "debug" });
        let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
        command
            .args(["build", "--package"])
            .arg(root)
            .arg("--out")
            .arg(&binary);
        if release {
            command.arg("--release");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(binary).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), expected);
    }
}

#[test]
fn canonical_package_aggregate_fields_match_source_in_debug_and_release() {
    for name in ["s2a-local", "s2a-imports"] {
        parity(&fixture(name), "42");
    }
}

#[test]
fn nullary_and_alternate_option_result_branches_match_source() {
    let temp = Temp::new();
    copy_fixture("s2a-local", &temp);
    let main = temp.0.join("src/main.cg");
    let source = fs::read_to_string(&main).unwrap()
        .replace("flag: Flag.Value(Point(x: 10))", "flag: Flag.Off()")
        .replace("maybe: Option<Flag>.Some(Flag.Off())", "maybe: Option<Flag>.None()")
        .replace("outcome: Result<Point, Flag>.Ok(Point(x: 8))", "outcome: Result<Point, Flag>.Err(Flag.Off())")
        .replace("nested: Option<Result<Point, Flag>>.Some(Result<Point, Flag>.Err(Flag.Value(Point(x: 5))))", "nested: Option<Result<Point, Flag>>.None()")
        .replace("reversed: Result<Option<Point>, Flag>.Ok(Option<Point>.Some(Point(x: 7)))", "reversed: Result<Option<Point>, Flag>.Err(Flag.Value(Point(x: 2)))");
    fs::write(main, source).unwrap();
    parity(&temp.0, "15");
}

#[test]
fn invalid_nominal_and_unknown_field_types_reject_before_publication() {
    let original = fs::read_to_string(fixture("s2a-imports").join("src/main.cg")).unwrap();
    for source in [
        original.replace("left: app.left.make(2)", "left: app.right.make(2)"),
        original.replace("app.right.Point(x: 4)", "app.left.Point(x: 4)"),
        original.replace("app.left.Flag.Off()", "app.right.Flag.Off()"),
        original.replace(
            "app.right.Flag.Value(app.right.Point(x: 5))",
            "app.left.Flag.Value(app.left.Point(x: 5))",
        ),
        original.replace("left: app.left.Holder", "left: app.left.Unknown"),
    ] {
        let temp = Temp::new();
        copy_fixture("s2a-imports", &temp);
        fs::write(temp.0.join("src/main.cg"), source).unwrap();
        assert!(run_native_package(&temp.0, &[]).is_err());
        for action in ["emit-rust", "build"] {
            let output = temp.0.join(format!("{action}-output"));
            fs::write(&output, "previous artifact").unwrap();
            let result = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
                .args([action, "--package"])
                .arg(&temp.0)
                .arg("--out")
                .arg(&output)
                .arg("--force")
                .output()
                .unwrap();
            assert!(!result.status.success());
            let errors = String::from_utf8_lossy(&result.stderr);
            assert!(errors.contains("CGT"), "{errors}");
            assert!(
                !errors.contains("CGXC005"),
                "unexpected Rust compilation: {errors}"
            );
            assert_eq!(fs::read_to_string(output).unwrap(), "previous artifact");
        }
    }
}
