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
            "cgx2-s2a-{}-{}",
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
const SOURCE: &str = include_str!("../../cgx2-syntax/s2a-evidence/fixtures/variants.cg");

fn build(path: &Path, package: bool, release: bool, out: &Path) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
    command.arg("build");
    if package {
        command.arg("--package");
    }
    command.arg(path).arg("--out").arg(out);
    if release {
        command.arg("--release");
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
fn nested_variants_contracts_and_checked_errors_match_native_debug_and_release() {
    let cases = [
        (SOURCE.to_owned(), Ok("42")),
        (SOURCE.replace("Reply.Value(41, true)", "Reply.Value(0, true)"), Err("CGR015")),
        (SOURCE.replace("x + 1", "9223372036854775807 + 1"), Err("CGR005")),
        (SOURCE.replace("ensures result > 0", "ensures result < 0"), Err("CGR016")),
        ("fn main() -> Option<Result<i64, Bool>>:\n    Option<Result<i64, Bool>>.Some(Result<i64, Bool>.Err(true))\n".into(),
         Ok("cgx2.core.Option<cgx2.core.Result<i64, Bool>>.Some(cgx2.core.Result<i64, Bool>.Err(true))")),
    ];
    for (source, expected) in cases {
        let temp = Temp::new();
        let input = temp.0.join("main.cg");
        fs::write(&input, &source).unwrap();
        let interpreter = cgx2_cli::run_checked_source(&source, &[]);
        for release in [false, true] {
            let output = temp.0.join(if release { "release" } else { "debug" });
            build(&input, false, release, &output);
            let result = Command::new(output).output().unwrap();
            match expected {
                Ok(expected) => {
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), expected);
                    assert_eq!(interpreter.as_ref().unwrap().0.value.to_string(), expected);
                }
                Err(code) => {
                    assert!(!result.status.success());
                    assert!(String::from_utf8_lossy(&result.stderr).contains(code));
                    assert_eq!(interpreter.as_ref().unwrap_err()[0].code, code);
                }
            }
        }
    }
}

#[test]
fn colliding_imported_variant_and_record_identities_match_native() {
    let root = Path::new("../cgx2-syntax/s2a-evidence/fixtures/package");
    assert_eq!(
        cgx2_cli::run_native_package(root, &[])
            .unwrap()
            .value
            .to_string(),
        "42"
    );
    let temp = Temp::new();
    for release in [false, true] {
        let output = temp.0.join(if release { "release" } else { "debug" });
        build(root, true, release, &output);
        let result = Command::new(output).output().unwrap();
        assert!(result.status.success());
        assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "42");
    }
}

#[test]
fn nullary_cases_none_error_branch_and_one_scrutinee_evaluation_match_native() {
    for (source, expected) in [
        (
            include_str!("../../cgx2-syntax/s2a-evidence/fixtures/evaluation-once.cg"),
            "200000",
        ),
        (
            include_str!("../../cgx2-syntax/s2a-evidence/fixtures/branches.cg"),
            "42",
        ),
    ] {
        assert_eq!(
            cgx2_cli::run_checked_source(source, &[])
                .unwrap()
                .0
                .value
                .to_string(),
            expected
        );
        let temp = Temp::new();
        let input = temp.0.join("main.cg");
        fs::write(&input, source).unwrap();
        for release in [false, true] {
            let output = temp.0.join(if release { "release" } else { "debug" });
            build(&input, false, release, &output);
            let result = Command::new(output).output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), expected);
        }
    }
}
