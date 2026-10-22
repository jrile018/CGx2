use cgx2_cli::run_checked_source;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case(PathBuf);

impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2c-memory-functions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn output(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn checked_memory_functions_match_interpreter_in_debug_and_release() {
    let case = Case::new();
    let fixtures = [
        ("memory_functions.cg", None, Some(1)),
        ("memory_functions_no_gc.cg", None, Some(0)),
        (
            "memory_functions_budget_failure.cg",
            Some("BudgetExceeded"),
            None,
        ),
        (
            "memory_functions_region_failure.cg",
            Some("RegionLimit"),
            None,
        ),
        (
            "memory_functions_conversion_failure.cg",
            Some("RegionLimit"),
            None,
        ),
    ];
    for (name, failure, collections) in fixtures {
        let source = fixture(name);
        let text = fs::read_to_string(&source).unwrap();
        let interpreted = run_checked_source(&text, &[]);
        match (&interpreted, failure) {
            (Ok((result, Some(report))), None) => {
                assert_eq!(result.value, cgx2_cli::Value::Unit);
                assert_eq!(report.totals.collection_count, collections.unwrap());
            }
            (Err(errors), Some(message)) => {
                assert_eq!(errors[0].code, "CGR013");
                assert!(errors[0].message.contains(message), "{errors:?}");
            }
            _ => panic!("unexpected interpreter result for {name}: {interpreted:?}"),
        }
        for release in [false, true] {
            let binary = case.output(&format!("{name}-{release}"));
            let mut build = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
            build.args(["build", path_str(&source), "--out", path_str(&binary)]);
            if release {
                build.arg("--release");
            }
            let built = build.output().unwrap();
            assert!(
                built.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&built.stderr)
            );
            let receipt = String::from_utf8(built.stdout).unwrap();
            assert!(receipt.contains(if release {
                "profile=release"
            } else {
                "profile=debug"
            }));
            assert!(receipt.contains("runtime_sha256="));
            let native = Command::new(&binary).output().unwrap();
            assert_eq!(
                native.status.code(),
                Some(if failure.is_some() { 1 } else { 0 })
            );
            assert!(native.stdout.is_empty());
            let stderr = String::from_utf8(native.stderr).unwrap();
            if let Some(message) = failure {
                let error = &interpreted.as_ref().unwrap_err()[0];
                let span = error.span.unwrap();
                assert!(stderr.contains(error.code), "{stderr}");
                assert!(stderr.contains(&error.message), "{stderr}");
                assert!(
                    stderr.contains(&format!("at {}..{}", span.start, span.end)),
                    "{stderr}"
                );
                assert!(stderr.contains(message), "{stderr}");
            } else {
                assert!(stderr.is_empty(), "{stderr}");
            }
        }
    }
}

#[test]
fn invalid_memory_functions_keep_existing_output_and_report_source_spans() {
    let case = Case::new();
    for name in [
        "memory_functions_dynamic_invalid.cg",
        "memory_functions_recursive_invalid.cg",
        "memory_functions_cross_call_invalid.cg",
        "memory_functions_foreign_invalid.cg",
    ] {
        let source = fixture(name);
        for mode in ["build", "emit-rust"] {
            let output = case.output(&format!("{name}-{mode}"));
            fs::write(&output, "existing output\n").unwrap();
            let result = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
                .args([
                    mode,
                    path_str(&source),
                    "--out",
                    path_str(&output),
                    "--force",
                ])
                .output()
                .unwrap();
            assert!(!result.status.success(), "{name} {mode}");
            let stderr = String::from_utf8(result.stderr).unwrap();
            assert!(stderr.contains(path_str(&source)), "{stderr}");
            assert!(stderr.contains(".."), "{stderr}");
            assert!(stderr.contains("CGM"), "{stderr}");
            assert_eq!(fs::read_to_string(&output).unwrap(), "existing output\n");
        }
    }
}
