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

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn moved_compiler_builds_memory_without_its_source_checkout() {
    let case = Case::new();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let checkout = case.0.join("disposable checkout");
    fs::create_dir(&checkout).unwrap();
    for file in ["Cargo.toml", "Cargo.lock"] {
        fs::copy(workspace.join(file), checkout.join(file)).unwrap();
    }
    copy_tree(&workspace.join("crates"), &checkout.join("crates"));
    let built = Command::new("cargo")
        .args(["build", "-p", "cgx2c", "--offline", "--manifest-path"])
        .arg(checkout.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(case.0.join("target"))
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", stderr(&built));
    let installed = case.0.join("installed cgx2c");
    fs::copy(case.0.join("target/debug/cgx2c"), &installed).unwrap();
    fs::remove_dir_all(checkout).unwrap();

    let source = case.source("memory.cg", include_str!("fixtures/memory_functions.cg"));
    let binary = case.0.join("memory program");
    let result = Command::new(installed)
        .args(["build", str_path(&source), "--out", str_path(&binary)])
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", stderr(&result));
    let report = case.0.join("moved-memory-report.json");
    let execution = Command::new(binary)
        .env("CGX2_MEMORY_REPORT", &report)
        .output()
        .unwrap();
    assert!(execution.status.success(), "{}", stderr(&execution));
    let json = fs::read_to_string(report).unwrap();
    assert!(json.contains("\"status\":\"ok\""));
    assert!(json.contains("\"collection_count\":1"));
}

#[cfg(unix)]
#[test]
fn memory_runtime_and_program_use_requested_profile_and_match_cli() {
    use std::os::unix::fs::PermissionsExt;

    let case = Case::new();
    let wrapper = case.0.join("logged rustc");
    fs::write(
        &wrapper,
        "#!/bin/sh\n{ printf 'BEGIN\\n'; printf '%s\\n' \"$@\"; printf 'END\\n'; } >> \"$CGX2C_ARG_LOG\"\nexec rustc \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();

    let fixtures = [
        (
            "manual.cg",
            "let r = memory.region(mode: manual, limit: 1048640)\nlet x = memory.alloc_bytes(r, 1048576)\nmemory.drop(x)\nmemory.collect(r)\nlet y = memory.alloc_bytes(r, 1048576)\nmemory.use(y)\n",
            true,
        ),
        (
            "no_gc.cg",
            "let r = memory.region(mode: manual, limit: 256)\nno_gc(budget: 95):\n    let x = memory.alloc_bytes(r, 32)\n",
            false,
        ),
    ];
    let mut identities = Vec::new();
    for release in [false, true] {
        for (name, text, succeeds) in fixtures {
            let source = case.source(name, text);
            let binary = case.0.join(format!("{name}-{release}"));
            let log = case.0.join(format!("args-{name}-{release}"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
            command
                .args([
                    "build",
                    str_path(&source),
                    "--out",
                    str_path(&binary),
                    "--rustc",
                    str_path(&wrapper),
                ])
                .env("CGX2C_ARG_LOG", &log);
            if release {
                command.arg("--release");
            }
            let built = command.output().unwrap();
            assert!(built.status.success(), "{}", stderr(&built));
            let receipt = String::from_utf8(built.stdout).unwrap();
            let identity = receipt
                .split_whitespace()
                .find_map(|part| part.strip_prefix("runtime_sha256="))
                .unwrap();
            assert_eq!(identity.len(), 64);
            identities.push(identity.to_owned());

            let arguments = fs::read_to_string(log).unwrap();
            let invocations: Vec<_> = arguments.split("BEGIN\n").skip(1).collect();
            assert_eq!(invocations.len(), 3, "{arguments}");
            let optimization = if release {
                "opt-level=3"
            } else {
                "opt-level=0"
            };
            assert!(invocations[0].contains("cgx2_runtime\n"), "{arguments}");
            assert!(
                invocations[0].contains(&format!("-C\n{optimization}\n")),
                "{arguments}"
            );
            assert!(invocations[1].contains("generated.rs\n"), "{arguments}");
            assert!(
                invocations[1].contains(&format!("-C\n{optimization}\n")),
                "{arguments}"
            );

            let cli = run_checked_source(text, &[]);
            let native = Command::new(binary).output().unwrap();
            assert_eq!(cli.is_ok(), succeeds);
            assert_eq!(native.status.success(), succeeds, "{}", stderr(&native));
            assert!(native.stdout.is_empty());
            if succeeds {
                assert!(native.stderr.is_empty());
            } else {
                assert!(stderr(&native).contains(&cli.unwrap_err()[0].message));
            }
        }
    }
    assert!(identities.iter().all(|identity| identity == &identities[0]));
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
fn native_memory_build_runs_existing_fixture_and_preserves_failure() {
    let case = Case::new();
    let fixture = include_str!("../../cgx2-cli/tests/fixtures/memory.cg");
    let source = case.source("memory.cg", fixture);
    let output = case.0.join("memory binary");
    let built = case.invoke(&["build", str_path(&source), "--out", str_path(&output)]);
    assert!(built.status.success(), "{}", stderr(&built));
    let actual = Command::new(&output).output().unwrap();
    assert!(actual.status.success(), "{}", stderr(&actual));
    assert!(actual.stdout.is_empty());
    assert!(actual.stderr.is_empty());

    let failing = "let r = memory.region(mode: manual, limit: 256)\nno_gc(budget: 95):\n    let x = memory.alloc_bytes(r, 32)\n";
    let source = case.source("failure.cg", failing);
    let output = case.0.join("failure binary");
    let built = case.invoke(&["build", str_path(&source), "--out", str_path(&output)]);
    assert!(built.status.success(), "{}", stderr(&built));
    let actual = Command::new(&output).output().unwrap();
    assert!(!actual.status.success());
    assert!(actual.stdout.is_empty());
    assert!(stderr(&actual).contains("CGR013"), "{}", stderr(&actual));
    assert!(stderr(&actual).contains("at "), "{}", stderr(&actual));
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
            "let r = memory.region(mode: manual, limit: 256)\nlet x = memory.alloc_bytes(r, 8)\nfor y in x:\n    memory.use(y)\n",
            "CGM001",
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

#[test]
fn package_build_does_not_replace_declared_binding_lock() {
    let case = Case::new();
    let source_dir = case.0.join("src");
    fs::create_dir(&source_dir).unwrap();
    fs::write(source_dir.join("main.cg"), "7\n").unwrap();
    fs::write(
        case.0.join("cgx2.toml"),
        "schema_version = 2\nname = \"sample\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\nbinding_lock = \"cgx2-bind.lock.json\"\n",
    )
    .unwrap();
    let lock = case.0.join("cgx2-bind.lock.json");
    fs::write(&lock, "preserve this lock\n").unwrap();
    let result = case.invoke(&[
        "build",
        "--package",
        str_path(&case.0),
        "--out",
        str_path(&lock),
        "--force",
    ]);
    assert!(!result.status.success());
    assert!(stderr(&result).contains("CGXC004"), "{}", stderr(&result));
    assert_eq!(fs::read_to_string(lock).unwrap(), "preserve this lock\n");
}
