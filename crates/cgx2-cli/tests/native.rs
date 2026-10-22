use cgx2_bind::{render_lock, Binding, BindingLock, Symbol};
use cgx2_cli::{check_native_package, run_native_package, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/native-c");

struct Fixture {
    root: PathBuf,
    lock: BindingLock,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::copy(format!("{FIXTURE}/cgx2.toml"), root.join("cgx2.toml")).unwrap();
        fs::copy(format!("{FIXTURE}/src/main.cg"), root.join("src/main.cg")).unwrap();
        let status = Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(root.join("libnative.so"))
            .arg(format!("{FIXTURE}/native.c"))
            .status()
            .unwrap();
        assert!(status.success());
        let hash = format!(
            "{:x}",
            Sha256::digest(fs::read(root.join("libnative.so")).unwrap())
        );
        let lock = BindingLock {
            schema_version: 1,
            package: "native_fixture".into(),
            bindings: vec![Binding {
                id: "fixture".into(),
                route: "native-c".into(),
                library: "native_fixture".into(),
                version: "1.0.0".into(),
                artifact: "libnative.so".into(),
                sha256: hash,
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                abi: "c".into(),
                calling_convention: "c".into(),
                layout: "status-i64-out-v1".into(),
                symbols: vec![Symbol {
                    name: "fixture_double".into(),
                    parameters: vec!["i64".into()],
                    result: "i64".into(),
                    ownership: "none".into(),
                    owner: None,
                    release: None,
                    error: "status".into(),
                    effects: vec!["foreign".into()],
                    capabilities: vec!["native".into()],
                }],
            }],
        };
        let fixture = Self { root, lock };
        fixture.write_lock();
        fixture
    }
    fn write_lock(&self) {
        fs::write(
            self.root.join("cgx2-bind.lock.json"),
            render_lock(&self.lock).unwrap(),
        )
        .unwrap();
    }
    fn source(&self, source: &str) {
        fs::write(self.root.join("src/main.cg"), source).unwrap();
    }
    fn read_source(&self) -> String {
        fs::read_to_string(self.root.join("src/main.cg")).unwrap()
    }
    fn code(&self) -> String {
        check_native_package(&self.root).unwrap_err()[0]
            .code
            .clone()
    }
}

#[test]
fn checked_source_calls_pinned_fixed_c_adapter_without_python() {
    let link_map = Command::new("ldd")
        .arg(env!("CARGO_BIN_EXE_cgx2-native"))
        .output()
        .unwrap();
    assert!(link_map.status.success());
    assert!(!String::from_utf8(link_map.stdout)
        .unwrap()
        .contains("libpython"));
    let fixture = Fixture::new();
    check_native_package(&fixture.root).unwrap();
    assert_eq!(
        run_native_package(&fixture.root, &[]).unwrap().value,
        Value::I64(42)
    );
    let checked = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .arg("check")
        .arg(&fixture.root)
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let ran = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .arg("run")
        .arg(&fixture.root)
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(ran.stdout, b"42\n");
    fixture.source(
        &fixture
            .read_source()
            .replace("double(21)", "double(21) + double(3)"),
    );
    assert_eq!(
        run_native_package(&fixture.root, &[]).unwrap().value,
        Value::I64(48)
    );
    fixture.source(
        &fixture
            .read_source()
            .replace("double(21) + double(3)", "double(0 - 1)"),
    );
    let error = run_native_package(&fixture.root, &[]).unwrap_err();
    assert_eq!(error[0].code, "CGR023");
    assert!(error[0].message.contains("ForeignStatus(17)"));
    let cli = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .arg("run")
        .arg(&fixture.root)
        .arg("--diagnostic-format=json")
        .output()
        .unwrap();
    assert!(!cli.status.success());
    assert!(cli.stdout.is_empty());
    assert!(String::from_utf8(cli.stderr).unwrap().contains("\"span\""));
}

#[test]
fn declaration_and_lock_rejections_are_source_located() {
    let mut fixture = Fixture::new();
    let original = fixture.read_source();
    for (source, code) in [
        (original.replace("unsafe ", ""), "CGT016"),
        (original.replace("effects(Foreign)", "effects()"), "CGT012"),
        (original.replace("caps(native)", "caps()"), "CGT013"),
        (
            original.replace("fixture.fixture_double", "missing.fixture_double"),
            "CGN002",
        ),
        (
            original.replace("fixture.fixture_double", "fixture.missing"),
            "CGN005",
        ),
        (
            original.replace("native_fixture@1.0.0", "native_fixture@2.0.0"),
            "CGN004",
        ),
    ] {
        fixture.source(&source);
        let errors = check_native_package(&fixture.root).unwrap_err();
        assert_eq!(errors[0].code, code, "{source}");
        assert!(errors[0].span.is_some());
    }
    fixture.source(&original);
    fixture.lock.bindings[0].sha256 = "0".repeat(64);
    fixture.write_lock();
    assert_eq!(fixture.code(), "CGB008");
    fixture.lock.bindings[0].sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(fixture.root.join("libnative.so")).unwrap())
    );
    fixture.lock.bindings[0].os = "wrong".into();
    fixture.write_lock();
    assert_eq!(fixture.code(), "CGB007");
    fixture.lock.bindings[0].os = std::env::consts::OS.into();
    fixture.lock.bindings[0].symbols[0]
        .parameters
        .push("i64".into());
    fixture.write_lock();
    assert_eq!(fixture.code(), "CGB017");
}

#[test]
fn missing_export_version_assertion_and_stale_source_fail_before_execution() {
    let mut fixture = Fixture::new();
    fixture.lock.bindings[0].symbols[0].name = "fixture_absent".into();
    fixture.write_lock();
    assert_eq!(fixture.code(), "CGB015");
    fixture.lock.bindings[0].symbols[0].name = "fixture_double".into();
    fixture.lock.bindings[0].version = "2.0.0".into();
    fixture.source(
        &fixture
            .read_source()
            .replace("native_fixture@1.0.0", "native_fixture@2.0.0"),
    );
    fixture.write_lock();
    assert_eq!(fixture.code(), "CGN007");
    fixture.lock.bindings[0].version = "1.0.0".into();
    fixture.source(
        &fixture
            .read_source()
            .replace("native_fixture@2.0.0", "native_fixture@1.0.0"),
    );
    fixture.write_lock();
    let checked = cgx2_package::load_checked_package(&fixture.root).unwrap();
    fixture.source(&fixture.read_source().replace("double(21)", "double(20)"));
    assert_eq!(checked.verify_unchanged().unwrap_err()[0].code, "CGP018");
}

#[test]
fn standalone_native_source_requires_package() {
    let fixture = Fixture::new();
    let source = fixture.read_source();
    assert_eq!(
        cgx2_cli::check_source(&source).unwrap_err()[0].code,
        "CGT017"
    );
    assert_eq!(
        cgx2_cli::run_checked_source(&source, &[]).unwrap_err()[0].code,
        "CGT017"
    );
    let path = Path::new(&fixture.root).join("src/main.cg");
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .arg("check")
        .arg(path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains("CGT017"));
}
