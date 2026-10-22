use cgx2_cli::run_checked_source;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Case(PathBuf);

impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2 package ;$ 世界 {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, body: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        path
    }
    fn manifest(&self, sources: &[&str]) {
        self.write(
            "cgx2.toml",
            &format!(
                "schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [{}]\n",
                sources
                    .iter()
                    .map(|s| format!("\"{s}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    fn invoke(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_cgx2c"))
            .args(args)
            .output()
            .unwrap()
    }
    fn build(&self, binary: &Path, release: bool) -> Output {
        let mut args = vec![
            "build",
            "--package",
            self.0.to_str().unwrap(),
            "--out",
            binary.to_str().unwrap(),
        ];
        if release {
            args.push("--release");
        }
        self.invoke(&args)
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn err(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn three_module_package_runs_in_debug_and_release_and_emits_deterministically() {
    let c = Case::new();
    c.manifest(&["src/main.cg", "src/math.cg", "src/base.cg"]);
    c.write(
        "src/main.cg",
        "module demo.main\nimport demo.math\nfn identity(value: i64) -> i64:\n    value\nfn main() -> i64:\n    identity(demo.math.twice(21))\n",
    );
    c.write("src/math.cg", "module demo.math\nimport demo.base\npub fn twice(x: i64) -> i64:\n    demo.base.add(x, x)\n");
    c.write(
        "src/base.cg",
        "module demo.base\npub fn add(x: i64, y: i64) -> i64:\n    x + y\n",
    );
    let rust = c.0.join("result ;$ 世界.rs");
    let emit = [
        "emit-rust",
        "--package",
        c.0.to_str().unwrap(),
        "--out",
        rust.to_str().unwrap(),
    ];
    assert!(c.invoke(&emit).status.success());
    let first = fs::read(&rust).unwrap();
    assert!(String::from_utf8_lossy(&first).contains("fn main()"));
    assert!(err(&c.invoke(&emit)).contains("CGXC004"));
    let mut forced = emit.to_vec();
    forced.push("--force");
    assert!(c.invoke(&forced).status.success());
    assert_eq!(fs::read(&rust).unwrap(), first);
    for release in [false, true] {
        let binary = c.0.join(format!("binary {release}"));
        let built = c.build(&binary, release);
        assert!(built.status.success(), "{}", err(&built));
        let receipt = String::from_utf8_lossy(&built.stdout);
        assert!(receipt.contains("package_manifest_sha256="));
        assert!(receipt.contains("src/math.cg:"));
        assert!(receipt.contains(if release {
            "profile=release"
        } else {
            "profile=debug"
        }));
        let result = Command::new(&binary).output().unwrap();
        assert!(result.status.success(), "{}", err(&result));
        assert_eq!(result.stdout, b"42\n");
        let equivalent = "fn add(left: i64, right: i64) -> i64:\n    left + right\nfn twice(value: i64) -> i64:\n    add(value, value)\nfn identity(value: i64) -> i64:\n    value\nfn main() -> i64:\n    identity(twice(21))\n";
        assert_eq!(
            run_checked_source(equivalent, &[])
                .unwrap()
                .0
                .value
                .to_string(),
            "42"
        );
    }
}

#[test]
fn package_call_depth_uses_standalone_limit() {
    let c = Case::new();
    c.manifest(&["src/main.cg"]);
    let mut source = String::from("module app\n");
    for n in 0..65 {
        source.push_str(&format!(
            "fn f{n}() -> i64:\n    {}\n",
            if n == 64 {
                "1".to_owned()
            } else {
                format!("f{}()", n + 1)
            }
        ));
    }
    source.push_str("fn main() -> i64:\n    f0()\n");
    c.write("src/main.cg", &source);
    let out = c.0.join("deep");
    let built = c.build(&out, false);
    assert!(built.status.success(), "{}", err(&built));
    let result = Command::new(&out).output().unwrap();
    assert!(!result.status.success());
    assert!(err(&result).contains("function call depth limit of 64 exceeded"));
}

#[test]
fn package_contracts_run_before_and_after_body_or_fail_with_span() {
    let c = Case::new();
    c.manifest(&["src/main.cg", "src/math.cg"]);
    c.write(
        "src/main.cg",
        "module demo.main\nimport demo.math\nfn main() -> i64:\n    demo.math.value(7)\n",
    );
    let math = |requires: &str, ensures: &str| {
        format!(
            "module demo.math\npub fn value(x: i64) -> i64 effects() caps():\n    requires {requires}\n    ensures {ensures}\n    x\n"
        )
    };
    for (i, (pre, post, expected)) in [
        ("x > 0", "result == x", None),
        ("false", "result == x", Some("requires condition failed")),
        ("true", "result == 8", Some("ensures condition failed")),
    ]
    .into_iter()
    .enumerate()
    {
        c.write("src/math.cg", &math(pre, post));
        let binary = c.0.join(format!("contract {i}"));
        let built = c.build(&binary, false);
        assert!(built.status.success(), "{}", err(&built));
        let output = Command::new(&binary).output().unwrap();
        match expected {
            Some(message) => {
                assert!(!output.status.success());
                assert!(err(&output).contains(message), "{}", err(&output));
                assert!(err(&output).contains("CGR01"));
                assert!(output.stdout.is_empty());
            }
            None => {
                assert!(output.status.success(), "{}", err(&output));
                assert_eq!(output.stdout, b"7\n");
            }
        }
    }
    c.write("src/math.cg", "module demo.math\nfn helper() -> Bool effects() caps():\n    true\npub fn value(x: i64) -> i64 effects() caps():\n    requires helper()\n    x\n");
    let rejected = c.build(&c.0.join("unsupported contract"), false);
    assert!(!rejected.status.success());
    assert!(err(&rejected).contains("CGRS001"), "{}", err(&rejected));
    assert!(
        err(&rejected).contains("contract expression is unsupported"),
        "{}",
        err(&rejected)
    );
    assert!(err(&rejected).contains(".cg:"), "{}", err(&rejected));
}

#[test]
fn private_missing_typed_cycle_and_memory_fail_before_rustc() {
    let c = Case::new();
    c.manifest(&["src/main.cg", "src/math.cg"]);
    c.write("src/math.cg", "module demo.math\nfn hidden(x: i64) -> i64:\n    x\npub fn public(x: i64) -> i64:\n    x\n");
    let binary = c.0.join("out");
    let cases = [
        (
            "module demo.main\nimport demo.math\nfn main() -> i64:\n    demo.math.hidden(2)\n",
            "CGT",
        ),
        (
            "module demo.main\nfn main() -> i64:\n    demo.math.public(2)\n",
            "CGT",
        ),
        (
            "module demo.main\nimport demo.math\nfn main() -> i64:\n    demo.math.public(true)\n",
            "CGT",
        ),
        (
            "module demo.main\nimport demo.math\nfn main() -> i64:\n    demo.math.public()\n",
            "CGT",
        ),
        (
            "module demo.main\nimport demo.absent\nfn main() -> i64:\n    1\n",
            "CGP014",
        ),
        (
            "module demo.main\nimport demo.math\nfn main() -> i64:\n    1\n",
            "CGP016",
        ),
        (
            "module demo.main\nimport demo.math\nfn main() -> i64:\n    memory.collect(1)\n",
            "CGM001",
        ),
        (
            "module demo.main\nfn main(x: i64) -> i64:\n    x\n",
            "CGRS001",
        ),
        ("module demo.main\nfn other() -> i64:\n    1\n", "CGRS001"),
    ];
    for (i, (body, code)) in cases.iter().enumerate() {
        c.write("src/main.cg", body);
        c.write("src/math.cg", if i == 5 { "module demo.math\nimport demo.main\npub fn public(x: i64) -> i64:\n    x\n" } else { "module demo.math\nfn hidden(x: i64) -> i64:\n    x\npub fn public(x: i64) -> i64:\n    x\n" });
        let result = c.build(&binary, false);
        assert!(!result.status.success(), "case {i}");
        assert!(err(&result).contains(code), "case {i}: {}", err(&result));
        assert!(err(&result).contains(".cg:"), "case {i}: {}", err(&result));
        assert!(!binary.exists());
    }
}

#[test]
fn package_entry_scalar_results_and_argument_error() {
    let c = Case::new();
    c.manifest(&["src/main.cg"]);
    for (i, (body, expected)) in [
        ("module app\nfn main() -> Bool:\n    true\n", "true\n"),
        (
            "module app\nfn main() -> String:\n    \"café 世界\"\n",
            "café 世界\n",
        ),
        ("module app\nfn main() -> Unit:\n    let x = 1\n", ""),
    ]
    .into_iter()
    .enumerate()
    {
        c.write("src/main.cg", body);
        let out = c.0.join(format!("scalar {i}"));
        let built = c.build(&out, false);
        assert!(built.status.success(), "{}", err(&built));
        assert_eq!(
            Command::new(&out).output().unwrap().stdout,
            expected.as_bytes()
        );
    }
    c.write(
        "src/main.cg",
        "module app\nfn main() -> String:\n    arg(0)\n",
    );
    let out = c.0.join("arg");
    assert!(c.build(&out, false).status.success());
    let result = Command::new(&out).output().unwrap();
    assert!(!result.status.success());
    assert!(err(&result).contains("arg index 0 is out of range"));
}

#[test]
fn dotted_names_keywords_and_runtime_errors_are_preserved() {
    let c = Case::new();
    c.manifest(&[
        "src/main.cg",
        "src/dotted.cg",
        "src/flat.cg",
        "src/builtin.cg",
    ]);
    c.write(
        "src/dotted.cg",
        "module a.b\npub fn value() -> i64:\n    20\n",
    );
    c.write(
        "src/flat.cg",
        "module a_b\npub fn value() -> i64:\n    22\n",
    );
    c.write(
        "src/builtin.cg",
        "module arg\npub fn value() -> i64:\n    0\n",
    );
    c.write(
        "src/main.cg",
        "module match\nimport a.b\nimport a_b\nimport arg\nfn main() -> i64:\n    a.b.value() + a_b.value() + arg.value()\n",
    );
    let binary = c.0.join("run");
    assert!(c.build(&binary, false).status.success());
    assert_eq!(Command::new(&binary).output().unwrap().stdout, b"42\n");
    for (source, error) in [
        (
            "module match\nimport a.b\nfn main() -> i64:\n    a.b.value() + 9223372036854775807\n",
            "i64 arithmetic overflow",
        ),
        (
            "module match\nimport a_b\nfn main() -> i64:\n    a_b.value() / 0\n",
            "division by zero",
        ),
    ] {
        c.write("src/main.cg", source);
        for release in [false, true] {
            let out = c.0.join(format!("runtime {release}"));
            let mut args = vec![
                "build",
                "--package",
                c.0.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
                "--force",
            ];
            if release {
                args.push("--release");
            }
            let built = c.invoke(&args);
            assert!(built.status.success(), "{}", err(&built));
            let result = Command::new(&out).output().unwrap();
            assert!(!result.status.success());
            assert!(err(&result).contains(error), "{}", err(&result));
        }
    }
}

#[test]
fn occupied_or_input_output_and_failed_compiler_preserve_files() {
    let c = Case::new();
    c.manifest(&["src/main.cg"]);
    let source = c.write(
        "src/main.cg",
        "module app\nfn main() -> String:\n    \"hello 世界\"\n",
    );
    let output = c.0.join("existing binary");
    fs::write(&output, b"existing").unwrap();
    assert!(err(&c.build(&output, false)).contains("CGXC004"));
    assert_eq!(fs::read(&output).unwrap(), b"existing");
    let same = c.invoke(&[
        "build",
        "--package",
        c.0.to_str().unwrap(),
        "--out",
        source.to_str().unwrap(),
        "--force",
    ]);
    assert!(err(&same).contains("CGXC004"));
    assert!(fs::read_to_string(&source).unwrap().contains("module app"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let fake = c.write(
            "fake rustc",
            "#!/bin/sh\necho intentional failure >&2\nexit 12\n",
        );
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let failed = c.invoke(&[
            "build",
            "--package",
            c.0.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
            "--force",
            "--rustc",
            fake.to_str().unwrap(),
        ]);
        assert!(err(&failed).contains("CGXC005"));
        assert_eq!(fs::read(&output).unwrap(), b"existing");
        assert!(fs::read_dir(&c.0).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".cgx2c-")
        }));
    }
}

#[cfg(unix)]
#[test]
fn source_changed_during_compilation_is_rejected_before_install() {
    use std::os::unix::fs::PermissionsExt;
    let c = Case::new();
    c.manifest(&["src/main.cg"]);
    let source = c.write("src/main.cg", "module app\nfn main() -> i64:\n    7\n");
    let wrapper = c.write(
        "mutating rustc",
        "#!/bin/sh\nprintf '\\n' >> \"$CGX2_TEST_MUTATE_PATH\"\nexec rustc \"$@\"\n",
    );
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let output = c.0.join("existing");
    fs::write(&output, "original").unwrap();
    let built = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
        .args([
            "build",
            "--package",
            c.0.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
            "--force",
            "--rustc",
            wrapper.to_str().unwrap(),
        ])
        .env("CGX2_TEST_MUTATE_PATH", &source)
        .output()
        .unwrap();
    assert!(!built.status.success());
    assert!(err(&built).contains("CGP018"), "{}", err(&built));
    assert_eq!(fs::read(&output).unwrap(), b"original");
    assert!(fs::read_dir(&c.0).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".cgx2c-")
    }));
}
