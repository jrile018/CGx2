use cgx2_cli::{Value, run_checked_source};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case(PathBuf);

impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2c test {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn source(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, text).unwrap();
        path
    }

    fn invoke(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_cgx2c"))
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn str_path(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn parsing_and_artifact_errors_are_distinct() {
    let case = Case::new();
    let source = case.source("input.cg", "1\n");
    let output = case.0.join("output.rs");
    assert!(stderr(&case.invoke(&["build", str_path(&source)])).contains("CGXC001"));
    assert!(
        stderr(&case.invoke(&["emit-rust", "missing.cg", "--out", str_path(&output)]))
            .contains("CGXC002")
    );
    let same = case.invoke(&[
        "build",
        str_path(&source),
        "--out",
        str_path(&source),
        "--force",
    ]);
    assert!(!same.status.success());
    assert!(stderr(&same).contains("CGXC004"));
    assert_eq!(fs::read_to_string(&source).unwrap(), "1\n");
}

#[test]
fn emit_is_deterministic_and_preserves_existing_output() {
    let case = Case::new();
    let source = case.source("hello 世界.cg", "\"café 世界\"\n");
    let output = case.0.join("emitted file.rs");
    let args = ["emit-rust", str_path(&source), "--out", str_path(&output)];
    let first = case.invoke(&args);
    assert!(first.status.success(), "{}", stderr(&first));
    let text = fs::read_to_string(&output).unwrap();
    assert!(text.contains("fn main()"));
    let collision = case.invoke(&args);
    assert!(stderr(&collision).contains("CGXC004"));
    assert_eq!(fs::read_to_string(&output).unwrap(), text);
    let manual = case.0.join("manual binary");
    let compiled = Command::new("rustc")
        .arg("--edition=2024")
        .arg("--crate-name")
        .arg("cgx2_generated")
        .arg(&output)
        .arg("-o")
        .arg(&manual)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{}", stderr(&compiled));
    assert_eq!(
        Command::new(&manual).output().unwrap().stdout,
        "café 世界\n".as_bytes()
    );
    let forced = case.invoke(&[
        "emit-rust",
        str_path(&source),
        "--out",
        str_path(&output),
        "--force",
    ]);
    assert!(forced.status.success(), "{}", stderr(&forced));
    assert_eq!(fs::read_to_string(&output).unwrap(), text);
}

#[test]
fn scalar_binary_matches_checked_cli_on_debug_and_release() {
    let case = Case::new();
    let fixtures = [
        ("2 + 3\n", vec![]),
        ("arg(0)\n", vec!["hello 世界"]),
        ("arg_count()\n", vec!["a", "b"]),
        ("\"café 世界\"\n", vec![]),
        ("9223372036854775807 + 1\n", vec![]),
        ("1 / 0\n", vec![]),
        ("arg(0)\n", vec![]),
    ];
    for (i, (text, args)) in fixtures.iter().enumerate() {
        let source = case.source(&format!("fixture {i}.cg"), text);
        let expected = run_checked_source(
            text,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        for release in [false, true] {
            let binary = case.0.join(format!("binary ;$ {i} {release}"));
            let mut command = vec!["build", str_path(&source), "--out", str_path(&binary)];
            if release {
                command.push("--release");
            }
            let build = case.invoke(&command);
            assert!(build.status.success(), "{}", stderr(&build));
            let receipt = String::from_utf8(build.stdout).unwrap();
            assert!(receipt.contains("source_sha256="));
            assert!(receipt.contains("toolchain=rustc "));
            assert!(receipt.contains("target="));
            let actual = Command::new(&binary).args(args).output().unwrap();
            match &expected {
                Ok((result, _)) => {
                    assert!(actual.status.success(), "{}", stderr(&actual));
                    let printed = if result.value == Value::Unit {
                        String::new()
                    } else {
                        format!("{}\n", result.value)
                    };
                    assert_eq!(actual.stdout, printed.as_bytes());
                    assert!(actual.stderr.is_empty());
                }
                Err(errors) => {
                    assert!(!actual.status.success());
                    assert!(actual.stdout.is_empty());
                    assert!(stderr(&actual).contains(&errors[0].message));
                }
            }
        }
    }
}

#[test]
fn diagnostics_and_failing_compiler_do_not_replace_output() {
    let case = Case::new();
    let output = case.0.join("existing binary");
    fs::write(&output, "existing").unwrap();
    for (source_text, code) in [
        ("let =\n", "CGS"),
        ("unknown\n", "CGT"),
        (
            "fn f(xs: List<i64>) -> i64:\n    for x in xs:\n        x\n    0\n\nfn main() -> i64:\n    0\n",
            "CGRS001",
        ),
        (
            "let r = memory.region(mode: manual, limit: 256)\n",
            "CGRS001",
        ),
    ] {
        let source = case.source("invalid.cg", source_text);
        let result = case.invoke(&[
            "build",
            str_path(&source),
            "--out",
            str_path(&output),
            "--force",
        ]);
        assert!(!result.status.success());
        assert!(stderr(&result).contains(code), "{}", stderr(&result));
        assert_eq!(fs::read_to_string(&output).unwrap(), "existing");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let symlink = case.0.join("linked output");
        std::os::unix::fs::symlink(&output, &symlink).unwrap();
        let source = case.source("valid.cg", "7\n");
        let linked = case.invoke(&[
            "build",
            str_path(&source),
            "--out",
            str_path(&symlink),
            "--force",
        ]);
        assert!(stderr(&linked).contains("CGXC004"));
        assert_eq!(fs::read_to_string(&output).unwrap(), "existing");
        let fake = case.0.join("fake rustc");
        fs::write(&fake, "#!/bin/sh\nprintf 'fake failure\\n' >&2\ni=0\nwhile [ \"$i\" -lt 1500 ]; do printf '0123456789' >&2; i=$((i+1)); done\nexit 42\n").unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let result = case.invoke(&[
            "build",
            str_path(&source),
            "--out",
            str_path(&output),
            "--force",
            "--rustc",
            str_path(&fake),
        ]);
        assert!(!result.status.success());
        assert!(stderr(&result).contains("CGXC005"));
        assert!(stderr(&result).contains("fake failure"));
        assert!(stderr(&result).contains("[stderr truncated]"));
        assert!(result.stderr.len() < 9000);
        assert_eq!(fs::read_to_string(&output).unwrap(), "existing");
        assert!(fs::read_dir(&case.0).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".cgx2c-")
        }));
    }
}
