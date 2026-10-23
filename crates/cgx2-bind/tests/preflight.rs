use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use cgx2_bind::{
    check_package, libtorch_facade_symbols, libtorch_matrix_facade_symbols, render_lock, Binding,
    BindingLock, Symbol,
};
use sha2::{Digest, Sha256};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new(template: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-bind-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("src")).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(template);
        fs::copy(source.join("cgx2.toml"), path.join("cgx2.toml")).unwrap();
        fs::copy(source.join("src/main.cg"), path.join("src/main.cg")).unwrap();
        Self(path)
    }

    fn write_lock(&self, lock: &BindingLock) {
        fs::write(
            self.0.join("cgx2-bind.lock.json"),
            render_lock(lock).unwrap(),
        )
        .unwrap();
    }

    fn c_fixture() -> (Self, BindingLock) {
        let fixture = Self::new("c-abi");
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/c-abi/native.c");
        let output = fixture.0.join("libfixture.so");
        assert!(Command::new("cc")
            .args(["-shared", "-fPIC"])
            .arg(source)
            .arg("-o")
            .arg(&output)
            .status()
            .unwrap()
            .success());
        let hash = format!("{:x}", Sha256::digest(fs::read(output).unwrap()));
        let lock = BindingLock {
            schema_version: 1,
            package: "native_fixture".into(),
            bindings: vec![Binding {
                id: "fixture".into(),
                route: "native-c".into(),
                library: "fixture".into(),
                version: "1.0.0".into(),
                artifact: "libfixture.so".into(),
                sha256: hash,
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                abi: "c".into(),
                calling_convention: "c".into(),
                layout: "scalar-v1".into(),
                symbols: vec![Symbol {
                    name: "fixture_add".into(),
                    parameters: vec!["i64".into(), "i64".into()],
                    result: "i64".into(),
                    ownership: "none".into(),
                    owner: None,
                    release: None,
                    error: "none".into(),
                    effects: vec!["foreign".into()],
                    capabilities: vec!["native".into()],
                }],
            }],
        };
        fixture.write_lock(&lock);
        (fixture, lock)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn codes(root: &Path) -> Vec<&'static str> {
    check_package(root)
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn opt_in_trace_preserves_success_and_first_failure_report() {
    let (fixture, _) = Fixture::c_fixture();
    for broken in [false, true] {
        if broken {
            fs::write(fixture.0.join("libfixture.so"), b"changed").unwrap();
        }
        let invoke = |trace: &str| {
            Command::new(env!("CARGO_BIN_EXE_cgx2-bind"))
                .arg("check")
                .arg(&fixture.0)
                .args(["--format", "json"])
                .env("CGX2_LOADER_TRACE", trace)
                .output()
                .unwrap()
        };
        let plain = invoke("0");
        let traced = invoke("1");
        let other = invoke("true");
        assert_eq!(plain.status.success(), !broken);
        assert_eq!(plain.status, traced.status);
        assert_eq!(plain.stdout, traced.stdout);
        assert_eq!(plain.stdout, other.stdout);
        assert!(plain.stderr.is_empty());
        assert!(other.stderr.is_empty());
        let diagnostic = String::from_utf8(traced.stderr).unwrap();
        assert!(diagnostic.contains("phase=package_preflight ns="));
        assert!(diagnostic
            .lines()
            .all(|line| line.starts_with("cgx2-loader pid=")));
    }
}

#[test]
fn c_fixture_passes_and_cli_outputs_stable_json() {
    if std::env::consts::OS != "linux" {
        return;
    }
    let (fixture, lock) = Fixture::c_fixture();
    let report = check_package(&fixture.0);
    assert!(report.passed(), "{report:?}");
    assert_eq!(render_lock(&lock).unwrap(), render_lock(&lock).unwrap());
    let parsed: BindingLock = serde_json::from_slice(&render_lock(&lock).unwrap()).unwrap();
    assert_eq!(render_lock(&parsed).unwrap(), render_lock(&lock).unwrap());
    let cli = Command::new(env!("CARGO_BIN_EXE_cgx2-bind"))
        .args(["check", fixture.0.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(cli.status.success());
    assert_eq!(
        String::from_utf8(cli.stdout).unwrap(),
        format!("{}\n", report.render_json())
    );
}

#[test]
fn rejects_hash_identity_symbol_and_missing_artifact() {
    if std::env::consts::OS != "linux" {
        return;
    }
    let (fixture, mut lock) = Fixture::c_fixture();
    lock.bindings[0].sha256 = "0".repeat(64);
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB008"));
    lock.bindings[0].sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(fixture.0.join("libfixture.so")).unwrap())
    );
    lock.bindings[0].symbols[0].name = "not_exported".into();
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB015"));
    lock.bindings[0].artifact = "missing.so".into();
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB014"));
    lock.package = "wrong_package".into();
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB004"));
}

#[test]
fn rejects_contract_platform_and_unknown_fields() {
    let (fixture, mut lock) = Fixture::c_fixture();
    lock.bindings[0].os = "neveros".into();
    lock.bindings[0].arch = "neverarch".into();
    lock.bindings[0].abi = "cpp".into();
    lock.bindings[0].version = ">=1.0".into();
    lock.bindings[0].calling_convention = "stdcall".into();
    lock.bindings[0].layout = "struct-v1".into();
    lock.bindings[0].symbols[0].parameters = vec!["struct Foo".into()];
    lock.bindings[0].symbols[0].effects = vec!["foreign".into(), "unknown".into()];
    lock.bindings[0].symbols[0].ownership = "owned".into();
    fixture.write_lock(&lock);
    let found = codes(&fixture.0);
    for code in ["CGB005", "CGB006", "CGB007", "CGB010", "CGB011", "CGB013"] {
        assert!(found.contains(&code), "missing {code}: {found:?}");
    }
    let mut value = serde_json::to_value(&lock).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("surprise".into(), true.into());
    fs::write(
        fixture.0.join("cgx2-bind.lock.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    assert!(codes(&fixture.0).contains(&"CGB003"));
}

#[test]
fn rejects_traversal_and_symlink_escape() {
    let (fixture, mut lock) = Fixture::c_fixture();
    lock.bindings[0].artifact = "../libfixture.so".into();
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB014"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = Fixture::new("c-abi");
        fs::copy(
            fixture.0.join("libfixture.so"),
            outside.0.join("libfixture.so"),
        )
        .unwrap();
        symlink(outside.0.join("libfixture.so"), fixture.0.join("escape.so")).unwrap();
        lock.bindings[0].artifact = "escape.so".into();
        fixture.write_lock(&lock);
        assert!(codes(&fixture.0).contains(&"CGB014"));
    }
}

#[test]
fn rejects_lock_schema_and_lock_symlink_escape() {
    let (fixture, mut lock) = Fixture::c_fixture();
    lock.schema_version = 3;
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB004"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = Fixture::new("c-abi");
        outside.write_lock(&lock);
        fs::remove_file(fixture.0.join("cgx2-bind.lock.json")).unwrap();
        symlink(
            outside.0.join("cgx2-bind.lock.json"),
            fixture.0.join("cgx2-bind.lock.json"),
        )
        .unwrap();
        assert!(codes(&fixture.0).contains(&"CGB002"));
    }
}

#[test]
fn rejects_excessive_binding_and_symbol_counts_before_inspection() {
    let (fixture, mut lock) = Fixture::c_fixture();
    lock.bindings = vec![lock.bindings[0].clone(); 33];
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB004"));

    lock.bindings.truncate(1);
    lock.bindings[0].symbols = vec![lock.bindings[0].symbols[0].clone(); 129];
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB004"));
}

#[test]
fn unavailable_libtorch_is_reported_without_fallback() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/libtorch-unavailable");
    let report = check_package(&fixture);
    assert!(!report.passed());
    assert!(report.diagnostics.iter().any(|d| d.code == "CGB014"));
    assert!(!report.render_json().contains("python"));
}

fn libtorch_mock_fixture(flags: &[&str]) -> (Fixture, BindingLock) {
    let fixture = Fixture::new("libtorch-unavailable");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cgx2-libtorch/facade/mock.c");
    let output = fixture.0.join("libcgx2_libtorch_facade.so");
    assert!(Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC"])
        .args(flags)
        .arg(source)
        .arg("-o")
        .arg(&output)
        .status()
        .unwrap()
        .success());
    let lock = BindingLock {
        schema_version: 1,
        package: "torch_unavailable".into(),
        bindings: vec![Binding {
            id: "torch_cpu".into(),
            route: "libtorch".into(),
            library: "cgx2.libtorch.cpu".into(),
            version: "2.6.0".into(),
            artifact: "libcgx2_libtorch_facade.so".into(),
            sha256: format!("{:x}", Sha256::digest(fs::read(output).unwrap())),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            abi: "cgx2-libtorch-facade-v1".into(),
            calling_convention: "c".into(),
            layout: "tensor-f64-cpu-v1".into(),
            symbols: libtorch_facade_symbols(),
        }],
    };
    fixture.write_lock(&lock);
    (fixture, lock)
}

#[test]
fn matrix_preflight_requires_exact_v2_exports_and_keeps_v1_valid() {
    let (fixture, mut lock) = libtorch_mock_fixture(&[]);
    assert!(check_package(&fixture.0).passed());
    lock.bindings[0].layout = "tensor-f64-cpu-matrix-v2".into();
    lock.bindings[0].symbols = libtorch_matrix_facade_symbols();
    fixture.write_lock(&lock);
    assert!(check_package(&fixture.0).passed());
    lock.bindings[0]
        .symbols
        .retain(|symbol| symbol.name != "cgx2_torch_matmul");
    fixture.write_lock(&lock);
    assert!(check_package(&fixture.0)
        .diagnostics
        .iter()
        .any(|d| d.code == "CGB018"));

    let (missing, mut missing_lock) = libtorch_mock_fixture(&["-DMOCK_MISSING_MATRIX_COPY"]);
    missing_lock.bindings[0].layout = "tensor-f64-cpu-matrix-v2".into();
    missing_lock.bindings[0].symbols = libtorch_matrix_facade_symbols();
    missing.write_lock(&missing_lock);
    assert!(check_package(&missing.0)
        .diagnostics
        .iter()
        .any(|d| d.code == "CGB015"));
    missing_lock.bindings[0].abi = "cgx2-libtorch-facade-v2".into();
    missing.write_lock(&missing_lock);
    assert!(check_package(&missing.0)
        .diagnostics
        .iter()
        .any(|d| d.code == "CGB006"));
}

#[test]
fn float32_matrix_preflight_requires_exact_v3_exports() {
    let (fixture, mut lock) = libtorch_mock_fixture(&[]);
    lock.bindings[0].layout = "tensor-f32-cpu-matrix-v3".into();
    lock.bindings[0].symbols = cgx2_bind::libtorch_f32_matrix_facade_symbols();
    fixture.write_lock(&lock);
    assert!(check_package(&fixture.0).passed());

    lock.bindings[0]
        .symbols
        .retain(|s| s.name != "cgx2_torch_matrix_f32_info");
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB018"));

    let (missing, mut lock) = libtorch_mock_fixture(&["-DMOCK_MISSING_F32_COPY"]);
    lock.bindings[0].layout = "tensor-f32-cpu-matrix-v3".into();
    lock.bindings[0].symbols = cgx2_bind::libtorch_f32_matrix_facade_symbols();
    missing.write_lock(&lock);
    assert!(codes(&missing.0).contains(&"CGB015"));
}

#[test]
fn mock_facade_preflight_pins_exact_bytes_and_c_signatures_without_loading() {
    let (fixture, mut lock) = libtorch_mock_fixture(&[]);
    let report = check_package(&fixture.0);
    assert!(report.passed(), "{}", report.render_text());
    lock.bindings[0].sha256 = "0".repeat(64);
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB008"));
    lock.bindings[0].sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(fixture.0.join("libcgx2_libtorch_facade.so")).unwrap())
    );
    lock.bindings[0].abi = "libtorch-stable".into();
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB006"));
    lock.bindings[0].abi = "cgx2-libtorch-facade-v1".into();
    lock.bindings[0].symbols[4].parameters = vec!["i64".into()];
    fixture.write_lock(&lock);
    assert!(codes(&fixture.0).contains(&"CGB018"));
}

#[test]
fn mock_facade_preflight_rejects_missing_export() {
    let (fixture, _) = libtorch_mock_fixture(&["-DMOCK_MISSING_COPY"]);
    assert!(codes(&fixture.0).contains(&"CGB015"));
}
