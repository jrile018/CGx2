use std::{fs, path::PathBuf, process::Command};

struct Temp(PathBuf);
impl Temp {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("cgx2-match-native-{name}-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn match_flow_matches_source_in_native_debug_and_release() {
    let temp = Temp::new("positive");
    let source_path = temp.0.join("main.cg");
    for source in [
        include_str!("../../cgx2-sema/tests/match_flow/nontail.cg"),
        include_str!("../../cgx2-sema/tests/match_flow/nontail_value.cg"),
        include_str!("../../cgx2-sema/tests/match_flow/all_return.cg"),
        include_str!("../../cgx2-sema/tests/match_flow/loop.cg"),
        include_str!("../../cgx2-sema/tests/match_flow/partial.cg"),
    ] {
        assert_eq!(
            cgx2_cli::run_checked_source(source, &[])
                .unwrap()
                .0
                .value
                .to_string(),
            "42"
        );
        fs::write(&source_path, source).unwrap();
        for release in [false, true] {
            let binary = temp.0.join(if release { "release" } else { "debug" });
            let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
            command
                .arg("build")
                .arg(&source_path)
                .arg("--out")
                .arg(&binary)
                .arg("--force");
            if release {
                command.arg("--release");
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let result = Command::new(&binary).output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "42");
        }
    }
}

#[test]
fn inconsistent_match_returns_fail_before_backend_publication() {
    let temp = Temp::new("negative");
    let source = temp.0.join("main.cg");
    fs::write(
        &source,
        include_str!("../../cgx2-sema/tests/match_flow/inferred.cg"),
    )
    .unwrap();
    for mode in ["build", "emit-rust"] {
        for release in [false, true] {
            let output = temp.0.join("bad-output");
            let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
            command.arg(mode).arg(&source).arg("--out").arg(&output);
            if release && mode == "build" {
                command.arg("--release");
            }
            let result = command.output().unwrap();
            assert!(!result.status.success());
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert!(stderr.contains("CGT003"), "{stderr}");
            assert!(!stderr.contains("CGXC005"), "{stderr}");
            assert!(!output.exists());
        }
    }
}
