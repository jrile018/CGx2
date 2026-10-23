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
        let root = std::env::temp_dir().join(format!(
            "cgx2-release-core-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn source(&self, name: &str, text: &str) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, text).unwrap();
        path
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_cgx2"))
            .current_dir(&self.0)
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
fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/release-core")
        .join(name)
}

#[test]
fn installed_product_build_emit_and_api_work_outside_checkout_without_helpers() {
    let case = Case::new();
    let installed = case.path("installed cgx2");
    fs::copy(env!("CARGO_BIN_EXE_cgx2"), &installed).unwrap();
    let source = fixture("scalar.cg");
    for release in [false, true] {
        let out = case.path(if release { "release" } else { "debug" });
        let mut command = Command::new(&installed);
        command
            .current_dir(&case.0)
            .args(["build", p(&source), "--out", p(&out)])
            .env("CGX2_PYTHON", "/missing/python")
            .env("CGX2_PYTHON_WORKER", "/missing/worker");
        if release {
            command.arg("--release");
        }
        let built = command.output().unwrap();
        success(&built);
        let result = Command::new(out).output().unwrap();
        success(&result);
        assert_eq!(result.stdout, b"25\n");
    }
    let out = case.path("script.py");
    success(
        &Command::new(&installed)
            .current_dir(&case.0)
            .args(["emit", p(&source), "--target", "python", "--out", p(&out)])
            .output()
            .unwrap(),
    );
    let python = Command::new("python3").arg(&out).output().unwrap();
    success(&python);
    assert_eq!(python.stdout, b"25\n");
    let api = Command::new(&installed)
        .current_dir(&case.0)
        .args(["api", "--json", p(&source)])
        .output()
        .unwrap();
    success(&api);
    let json: serde_json::Value = serde_json::from_slice(&api.stdout).unwrap();
    assert!(json.to_string().contains("map_one"));
    assert!(!case.path("cgx2c").exists());
}

#[test]
fn checked_package_targets_are_deterministic_and_match_existing_execution() {
    let case = Case::new();
    let root = fixture("package");
    let mut outputs = Vec::new();
    for target in ["rust", "python"] {
        let out = case.path(if target == "rust" {
            "package.rs"
        } else {
            "package.pyz"
        });
        success(&case.run(&["emit", p(&root), "--target", target, "--out", p(&out)]));
        let bytes = fs::read(&out).unwrap();
        success(&case.run(&[
            "emit",
            p(&root),
            "--target",
            target,
            "--out",
            p(&out),
            "--force",
        ]));
        assert_eq!(bytes, fs::read(&out).unwrap());
        let output = if target == "rust" {
            let bin = case.path("rust-emitted");
            success(
                &Command::new("rustc")
                    .args(["--edition=2024", p(&out), "-o", p(&bin)])
                    .output()
                    .unwrap(),
            );
            Command::new(bin).output().unwrap()
        } else {
            // Python validates CRCs and inspects the actual, editable source members.
            success(&Command::new("python3").args(["-c", "import sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None; assert '__main__.py' in z.namelist(); assert all(i.date_time==(1980,1,1,0,0,0) for i in z.infolist())", p(&out)]).output().unwrap());
            Command::new("python3").arg(out).output().unwrap()
        };
        success(&output);
        outputs.push(output.stdout);
    }
    for release in [false, true] {
        let bin = case.path(if release {
            "native-release"
        } else {
            "native-debug"
        });
        let mut args = vec!["build", p(&root), "--out", p(&bin)];
        if release {
            args.push("--release");
        }
        success(&case.run(&args));
        let output = Command::new(bin).output().unwrap();
        success(&output);
        outputs.push(output.stdout);
    }
    assert!(outputs.iter().all(|o| o == b"25\n"));
    let old = case.run(&["api", p(&root), "--json"]);
    success(&old);
    let new = case.run(&["api", "--json", p(&root)]);
    success(&new);
    assert_eq!(old.stdout, new.stdout);
    let api: serde_json::Value = serde_json::from_slice(&new.stdout).unwrap();
    assert_eq!(api["modules"].as_array().unwrap().len(), 2);
    assert!(!String::from_utf8(new.stdout).unwrap().contains(p(&root)));
}

#[test]
fn parse_type_target_and_compiler_failures_preserve_existing_artifact() {
    let case = Case::new();
    let out = case.source("existing", "preserved");
    for (text, target) in [
        ("let =\n", "rust"),
        ("unknown\n", "python"),
        ("1\n", "gpu"),
        (
            "let r = memory.region(mode: manual, limit: 256)\n",
            "python",
        ),
    ] {
        let source = case.source("input.cg", text);
        let result = case.run(&[
            "--diagnostic-format=json",
            "emit",
            p(&source),
            "--target",
            target,
            "--out",
            p(&out),
            "--force",
        ]);
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        serde_json::from_slice::<serde_json::Value>(&result.stderr).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"preserved");
    }
    let source = case.source("input.cg", "1\n");
    let result = case.run(&[
        "--diagnostic-format=json",
        "build",
        p(&source),
        "--out",
        p(&out),
        "--force",
        "--rustc",
        "/missing/rustc",
    ]);
    assert_eq!(result.status.code(), Some(1));
    let diagnostic: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(diagnostic["code"], "CGXC005");
    assert_eq!(fs::read(&out).unwrap(), b"preserved");
    assert!(fs::read_dir(&case.0).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".cgx2c-")));
}

#[test]
fn unicode_diagnostics_use_source_bytes_and_package_relative_paths() {
    let case = Case::new();
    let text = "module bad.main\n# café 世界\nfn main() -> i64:\n    missing\n";
    let source = case.source("main.cg", text);
    case.source(
        "cgx2.toml",
        "schema_version = 1\nname = \"bad\"\nentry = \"main.cg\"\nsources = [\"main.cg\"]\n",
    );
    let out = case.source("existing.pyz", "preserved");
    for mode in ["api", "build", "emit"] {
        let mut args = vec!["--diagnostic-format=json", mode];
        if mode == "api" {
            args.push("--json");
        }
        args.push(p(&case.0));
        if mode != "api" {
            args.extend(["--out", p(&out), "--force"]);
        }
        if mode == "emit" {
            args.extend(["--target", "python"]);
        }
        let output = case.run(&args);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let d: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(d["file"], "main.cg");
        assert_eq!(d["span"]["start"], text.find("missing").unwrap());
        assert_eq!(d["span"]["end"], text.find("missing").unwrap() + 7);
        assert_eq!(fs::read(&out).unwrap(), b"preserved");
    }
    let output = case.run(&[
        "--diagnostic-format=json",
        "build",
        p(&source),
        "--out",
        p(&out),
        "--force",
    ]);
    let d: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(d["span"]["start"], text.find("missing").unwrap());
}

#[cfg(unix)]
#[test]
fn outputs_never_overwrite_sources_links_manifests_or_directories() {
    let case = Case::new();
    let text = "7\n";
    let source = case.source("input.cg", text);
    let hard = case.path("hard");
    fs::hard_link(&source, &hard).unwrap();
    let link = case.path("link");
    std::os::unix::fs::symlink(&source, &link).unwrap();
    let directory = case.path("directory");
    fs::create_dir(&directory).unwrap();
    for target in [&source, &hard, &link, &directory] {
        let result = case.run(&[
            "emit",
            p(&source),
            "--target",
            "rust",
            "--out",
            p(target),
            "--force",
        ]);
        assert!(!result.status.success());
        assert_eq!(fs::read(&source).unwrap(), text.as_bytes());
    }
    let root = fixture("package");
    let math = root.join("math.cg");
    let original = fs::read(&math).unwrap();
    let result = case.run(&[
        "emit",
        p(&root),
        "--target",
        "rust",
        "--out",
        p(&math),
        "--force",
    ]);
    assert!(!result.status.success());
    assert_eq!(fs::read(&math).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn relative_package_revalidation_diagnostics_stay_relative_and_preserve_output() {
    use std::os::unix::fs::PermissionsExt;
    let case = Case::new();
    case.source("main.cg", "module app\nfn main() -> i64:\n    7\n");
    case.source(
        "cgx2.toml",
        "schema_version = 1\nname = \"app\"\nentry = \"main.cg\"\nsources = [\"main.cg\"]\n",
    );
    let rustc = case.source(
        "mutating-rustc",
        "#!/bin/sh\nprintf '\\n' >> main.cg\nexec rustc \"$@\"\n",
    );
    fs::set_permissions(&rustc, fs::Permissions::from_mode(0o700)).unwrap();
    let out = case.source("existing", "preserve me");
    let result = case.run(&[
        "--diagnostic-format=json",
        "build",
        ".",
        "--out",
        p(&out),
        "--force",
        "--rustc",
        p(&rustc),
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    let diagnostic: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(diagnostic["code"], "CGP018");
    assert_eq!(diagnostic["file"], "main.cg");
    assert!(diagnostic["span"].is_null());
    assert_eq!(fs::read(out).unwrap(), b"preserve me");
}
