use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("cgx2c-checked-effects-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../cgx2-package/fixtures/checked-effects");
        fs::copy(fixture.join("cgx2.toml"), root.join("cgx2.toml")).unwrap();
        for name in ["main", "join", "left", "right", "base"] {
            fs::copy(
                fixture.join(format!("src/{name}.cg")),
                root.join(format!("src/{name}.cg")),
            )
            .unwrap();
        }
        Self(root)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn checked_diamond_matches_interpreter_rust_native_profiles_and_atomic_rejection() {
    let fixture = Fixture::new();
    let base = fixture.0.join("src/base.cg");
    let pure = fs::read_to_string(&base).unwrap();
    for (source, failure) in [(&pure, false), (&pure.replace("x + 1", "x / 0"), true)] {
        fs::write(&base, source).unwrap();
        let interpreted = cgx2_cli::run_native_package(&fixture.0, &[]);
        if failure {
            assert_eq!(interpreted.unwrap_err()[0].code, "CGR006");
        } else {
            assert_eq!(interpreted.unwrap().value.to_string(), "42");
        }
        let rust = fixture.0.join("program.rs");
        let emitted = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
            .args(["emit-rust", "--package"])
            .arg(&fixture.0)
            .arg("--out")
            .arg(&rust)
            .arg("--force")
            .output()
            .unwrap();
        assert!(
            emitted.status.success(),
            "{}",
            String::from_utf8_lossy(&emitted.stderr)
        );
        // Compile the emitted source independently of cgx2c's native build route.
        let standalone = fixture.0.join("standalone");
        let compiled = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&standalone)
            .arg(&rust)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        for release in [false, true] {
            let binary = fixture.0.join(if release { "release" } else { "debug" });
            let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
            command
                .args(["build", "--package"])
                .arg(&fixture.0)
                .arg("--out")
                .arg(&binary)
                .arg("--force");
            if release {
                command.arg("--release");
            }
            let compiled = command.output().unwrap();
            assert!(
                compiled.status.success(),
                "{}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            for program in [&binary, &standalone] {
                let result = Command::new(program).output().unwrap();
                assert_eq!(result.status.success(), !failure);
                if failure {
                    assert!(String::from_utf8_lossy(&result.stderr).contains("CGR006"));
                } else {
                    assert_eq!(result.stdout, b"42\n");
                }
            }
        }
    }
    fs::write(&base, pure.replace("x + 1", "arg_count()")).unwrap();
    assert!(cgx2_cli::run_native_package(&fixture.0, &[]).is_err());
    for mode in ["emit-rust", "build"] {
        let output = fixture.0.join("preserved-output");
        fs::write(&output, "previous artifact").unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
            .args([mode, "--package"])
            .arg(&fixture.0)
            .arg("--out")
            .arg(&output)
            .arg("--force")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("CGT012"));
        assert_eq!(fs::read_to_string(&output).unwrap(), "previous artifact");
    }
}
