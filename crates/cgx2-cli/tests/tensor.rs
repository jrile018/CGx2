use cgx2_bind::{
    libtorch_f32_matrix_facade_symbols, libtorch_facade_symbols, libtorch_matrix_facade_symbols,
    render_lock, Binding, BindingLock, ClosureLock, LibraryPin,
};
use cgx2_cli::{check_native_package, run_native_package, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static NEXT: AtomicUsize = AtomicUsize::new(0);
static TEST_LOCK: Mutex<()> = Mutex::new(());
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tensor-cpu");
const DEP: &str = "libcgx2_closure_dep.so";
const MATRIX_SOURCE: &str = include_str!("fixtures/matrix-cpu/main.cg");
const MATRIX_F32_SOURCE: &str = include_str!("fixtures/matrix-f32-cpu/main.cg");

struct Fixture {
    root: PathBuf,
    lock: BindingLock,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-tensor-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::copy(format!("{FIXTURE}/cgx2.toml"), root.join("cgx2.toml")).unwrap();
        fs::copy(format!("{FIXTURE}/src/main.cg"), root.join("src/main.cg")).unwrap();
        let mock = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../cgx2-libtorch/facade/mock.c"
        );
        let dep = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../cgx2-libtorch/fixtures/closure/dep.c"
        );
        assert!(Command::new("cc")
            .args([
                "-shared",
                "-fPIC",
                "-nostdlib",
                "-Wl,-soname,libcgx2_closure_dep.so",
            ])
            .arg(dep)
            .arg("-o")
            .arg(root.join(DEP))
            .status()
            .unwrap()
            .success());
        assert!(Command::new("cc")
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-shared",
                "-fPIC",
                "-nostdlib",
                "-Wl,-soname,libmock.so",
                "-Wl,-rpath,$ORIGIN",
            ])
            .arg(mock)
            .arg("-L")
            .arg(&root)
            .args(["-Wl,--no-as-needed", "-lcgx2_closure_dep"])
            .arg("-o")
            .arg(root.join("libmock.so"))
            .status()
            .unwrap()
            .success());
        let lock = BindingLock {
            schema_version: 2,
            package: "tensor_cpu_fixture".into(),
            bindings: vec![Binding {
                id: "torch_cpu".into(),
                route: "libtorch".into(),
                library: "cgx2.libtorch.cpu".into(),
                version: "2.6.0".into(),
                artifact: "libmock.so".into(),
                sha256: format!(
                    "{:x}",
                    Sha256::digest(fs::read(root.join("libmock.so")).unwrap())
                ),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                abi: "cgx2-libtorch-facade-v1".into(),
                calling_convention: "c".into(),
                layout: "tensor-f64-cpu-v1".into(),
                symbols: libtorch_facade_symbols(),
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
        let libraries = [("facade", "libmock.so"), ("dependency", DEP)]
            .into_iter()
            .map(|(role, name)| LibraryPin {
                role: role.into(),
                name: name.into(),
                path: self.root.join(name).canonicalize().unwrap(),
                sha256: format!(
                    "{:x}",
                    Sha256::digest(fs::read(self.root.join(name)).unwrap())
                ),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                soname: Some(name.into()),
            })
            .collect();
        fs::write(
            self.root.join("cgx2-libtorch-closure.lock.json"),
            serde_json::to_vec_pretty(&ClosureLock {
                schema_version: 1,
                libraries,
            })
            .unwrap(),
        )
        .unwrap();
    }
    fn matrix() -> Self {
        let mut fixture = Self::new();
        fixture.set_source(MATRIX_SOURCE);
        fixture.lock.bindings[0].layout = "tensor-f64-cpu-matrix-v2".into();
        fixture.lock.bindings[0].symbols = libtorch_matrix_facade_symbols();
        fixture.write_lock();
        fixture
    }
    fn matrix_f32() -> Self {
        let mut fixture = Self::new();
        fixture.set_source(MATRIX_F32_SOURCE);
        fixture.lock.bindings[0].layout = "tensor-f32-cpu-matrix-v3".into();
        fixture.lock.bindings[0].symbols = libtorch_f32_matrix_facade_symbols();
        fixture.write_lock();
        fixture
    }
    fn source(&self) -> String {
        fs::read_to_string(self.root.join("src/main.cg")).unwrap()
    }
    fn set_source(&self, source: &str) {
        fs::write(self.root.join("src/main.cg"), source).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn checked_cg_source_runs_mock_cpu_facade_without_python() {
    let _guard = TEST_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    check_native_package(&fixture.root).unwrap();
    assert_eq!(
        run_native_package(&fixture.root, &[]).unwrap().value,
        Value::String("[4.0, 6.0]".into())
    );
    let checked = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["check", fixture.root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let ran = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["run", fixture.root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(ran.stdout, b"[4.0, 6.0]\n");
    let linked = Command::new("ldd")
        .arg(env!("CARGO_BIN_EXE_cgx2-native"))
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&linked.stdout).contains("libpython"));
}

#[test]
fn source_rejections_and_artifact_errors_are_located() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new();
    let original = fixture.source();
    for (source, code) in [
        (original.replace("unsafe ", ""), "CGT018"),
        (original.replace("effects(Foreign)", "effects()"), "CGT018"),
        (original.replace("caps(native)", "caps()"), "CGT018"),
        (original.replace("[3.0, 4.0]", "[3.0]"), "CGT018"),
        (original.replace("[3.0, 4.0]", "[[3.0, 4.0]]"), "CGT018"),
        (original.replace("[3.0, 4.0]", "[NaN, 4.0]"), "CGT018"),
        (
            original.replace(
                "add_f64_cpu(\"[1.0, 2.0]\", \"[3.0, 4.0]\")",
                "let escaped = add_f64_cpu(\"[1.0, 2.0]\", \"[3.0, 4.0]\")\n    escaped",
            ),
            "CGT018",
        ),
    ] {
        fixture.set_source(&source);
        let errors = check_native_package(&fixture.root).unwrap_err();
        assert_eq!(errors[0].code, code, "{source}: {errors:?}");
        assert!(errors[0].span.is_some());
    }
    fixture.set_source(include_str!("fixtures/tensor-cpu/bad_shape.cg"));
    let mismatch = check_native_package(&fixture.root).unwrap_err();
    assert_eq!(mismatch[0].code, "CGT018");
    assert!(mismatch[0].message.contains("equal one-dimensional shapes"));
    let oversized = format!("[{}]", vec!["1.0"; 1025].join(", "));
    fixture.set_source(&original.replace("[3.0, 4.0]", &oversized));
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGT018"
    );
    fixture.set_source(&original);
    fixture.lock.bindings[0].sha256 = "0".repeat(64);
    fixture.write_lock();
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGB008"
    );
    fixture.lock.bindings[0].sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(fixture.root.join("libmock.so")).unwrap())
    );
    fixture.lock.bindings[0].version = "wrong".into();
    fixture.write_lock();
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGB005"
    );
    fixture.lock.bindings[0].version = "2.6.0".into();
    fixture.lock.bindings[0].artifact = "missing.so".into();
    fixture.write_lock();
    assert!(!check_native_package(&fixture.root).unwrap_err().is_empty());
    fixture.lock.bindings[0].artifact = "libmock.so".into();
    fixture.lock.schema_version = 1;
    fixture.write_lock();
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGN004"
    );
    fixture.lock.schema_version = 2;
    fixture.write_lock();
    fs::write(fixture.root.join(DEP), b"changed dependency").unwrap();
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGB019"
    );
}

#[test]
fn nonfinite_result_is_rejected_after_foreign_copy() {
    let _guard = TEST_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    fixture.set_source(
        &fixture
            .source()
            .replace("[1.0, 2.0]", "[1e308, 1e308]")
            .replace("[3.0, 4.0]", "[1e308, 1e308]"),
    );
    check_native_package(&fixture.root).unwrap();
    let errors = run_native_package(&fixture.root, &[]).unwrap_err();
    assert_eq!(errors[0].code, "CGR024");
    assert!(errors[0].span.is_some());
}

#[test]
fn checked_matrix_source_runs_pinned_v2_facade_repeatedly() {
    let _guard = TEST_LOCK.lock().unwrap();
    let fixture = Fixture::matrix();
    check_native_package(&fixture.root).unwrap();
    let checked = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["check", fixture.root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    for _ in 0..12 {
        assert_eq!(
            run_native_package(&fixture.root, &[]).unwrap().value,
            Value::String("[[58.0, 64.0], [139.0, 154.0]]".into())
        );
    }
    let ran = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["run", fixture.root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(ran.stdout, b"[[58.0, 64.0], [139.0, 154.0]]\n");
}

#[test]
fn matrix_source_and_binding_errors_are_located_before_loading() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::matrix();
    let original = fixture.source();
    let left = "[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]";
    let right = "[[7.0, 8.0], [9.0, 10.0], [11.0, 12.0]]";
    for (literal, message) in [
        ("[]", "rows"),
        ("[[]]", "columns"),
        ("[[1.0], [2.0, 3.0]]", "equal lengths"),
        ("[[[1.0]]]", "rank two"),
        ("[[NaN]]", "finite"),
        ("[[1e309]]", "Infinity"),
        ("[[1.0], [2.0]]", "left columns"),
        ("[[1.0],]", "trailing comma"),
    ] {
        fixture.set_source(&original.replace(left, literal));
        let errors = check_native_package(&fixture.root).unwrap_err();
        assert_eq!(errors[0].code, "CGT018", "{literal}: {errors:?}");
        assert!(errors[0].message.contains(message), "{literal}: {errors:?}");
        assert!(errors[0].span.is_some());
    }
    let too_many_columns = format!("[[{}]]", vec!["1.0"; 33].join(", "));
    fixture.set_source(&original.replace(left, &too_many_columns));
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGT018"
    );
    let too_many_rows = format!("[{}]", vec!["[1.0]"; 33].join(", "));
    fixture.set_source(&original.replace(left, &too_many_rows));
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGT018"
    );
    fixture.set_source(&original.replace(right, "[[1.0, 2.0]]"));
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGT018"
    );
    fixture.set_source(&original);
    fixture.lock.bindings[0].layout = "tensor-f64-cpu-v1".into();
    fixture.lock.bindings[0].symbols = libtorch_facade_symbols();
    fixture.write_lock();
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGN004"
    );
    fixture.lock.bindings[0].layout = "tensor-f64-cpu-matrix-v2".into();
    fixture.lock.bindings[0].symbols = libtorch_matrix_facade_symbols();
    fixture.lock.bindings[0].route = "native-c".into();
    fixture.write_lock();
    assert!(!check_native_package(&fixture.root).unwrap_err().is_empty());
    fixture.lock.bindings[0].route = "libtorch".into();
    fixture.write_lock();
    fixture.lock.bindings[0].abi = "wrong".into();
    fixture.write_lock();
    assert!(!check_native_package(&fixture.root).unwrap_err().is_empty());
    fixture.lock.bindings[0].abi = "cgx2-libtorch-facade-v1".into();
    fixture.write_lock();
    fs::remove_file(fixture.root.join("cgx2-bind.lock.json")).unwrap();
    assert!(!check_native_package(&fixture.root).unwrap_err().is_empty());
}

#[test]
fn matrix_runtime_rejects_nonfinite_output_and_missing_artifact() {
    let _guard = TEST_LOCK.lock().unwrap();
    let fixture = Fixture::matrix();
    fixture.set_source(&fixture.source().replace(
        "[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]",
        "[[1e308, 1e308, 1e308]]",
    ));
    check_native_package(&fixture.root).unwrap();
    let errors = run_native_package(&fixture.root, &[]).unwrap_err();
    assert_eq!(errors[0].code, "CGR024");
    assert!(errors[0].span.is_some());
    fixture.set_source(MATRIX_SOURCE);
    fs::remove_file(fixture.root.join("libmock.so")).unwrap();
    assert!(!check_native_package(&fixture.root).unwrap_err().is_empty());
}

#[test]
fn real_pinned_matrix_source_when_available() {
    let Ok(root) = std::env::var("CGX2_LIBTORCH_SOURCE_PACKAGE") else {
        eprintln!("skipped: set CGX2_LIBTORCH_SOURCE_PACKAGE to a reviewed schema-2 matrix source package");
        return;
    };
    let root = PathBuf::from(root);
    check_native_package(&root).unwrap();
    assert_eq!(
        run_native_package(&root, &[]).unwrap().value,
        Value::String("[[58.0, 64.0], [139.0, 154.0]]".into())
    );
}

#[test]
fn matrix_profile_rejects_missing_and_multiple_source_packages() {
    let _guard = TEST_LOCK.lock().unwrap();
    let fixture = Fixture::matrix();
    assert!(!check_native_package(&fixture.root.join("absent"))
        .unwrap_err()
        .is_empty());
    fixture.set_source(&format!(
        "module tensor_cpu_fixture.main\n\n{MATRIX_SOURCE}"
    ));
    fs::write(
        fixture.root.join("src/extra.cg"),
        "module tensor_cpu_fixture.extra\n\nfn helper() -> i64:\n    1\n",
    )
    .unwrap();
    let manifest = fs::read_to_string(fixture.root.join("cgx2.toml")).unwrap();
    fs::write(
        fixture.root.join("cgx2.toml"),
        manifest.replace(
            "sources = [\"src/main.cg\"]",
            "sources = [\"src/main.cg\", \"src/extra.cg\"]",
        ),
    )
    .unwrap();
    let errors = check_native_package(&fixture.root).unwrap_err();
    assert_eq!(errors[0].code, "CGN001");
    assert!(errors[0].span.is_some());
}

#[test]
fn checked_float32_matrix_source_runs_mock_v3_facade_repeatedly() {
    let _guard = TEST_LOCK.lock().unwrap();
    let fixture = Fixture::matrix_f32();
    check_native_package(&fixture.root).unwrap();
    let checked = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["check", fixture.root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    for _ in 0..12 {
        assert_eq!(
            run_native_package(&fixture.root, &[]).unwrap().value,
            Value::String("[[58.0, 64.0], [139.0, 154.0]]".into())
        );
    }
    let ran = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["run", fixture.root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(ran.stdout, b"[[58.0, 64.0], [139.0, 154.0]]\n");
    let linked = Command::new("ldd")
        .arg(env!("CARGO_BIN_EXE_cgx2-native"))
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&linked.stdout).contains("libpython"));
}

#[test]
fn float32_source_rejects_layout_version_dtype_and_nonfinite_results() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::matrix_f32();
    fixture.lock.bindings[0].layout = "tensor-f64-cpu-matrix-v2".into();
    fixture.write_lock();
    let errors = check_native_package(&fixture.root).unwrap_err();
    assert_eq!(errors[0].code, "CGB018");
    assert!(errors[0].span.is_some());
    fixture.lock.bindings[0].layout = "tensor-f32-cpu-matrix-v3".into();
    fixture.lock.bindings[0].version = "wrong".into();
    fixture.write_lock();
    assert_eq!(
        check_native_package(&fixture.root).unwrap_err()[0].code,
        "CGB005"
    );
    fixture.lock.bindings[0].version = "2.6.0".into();
    fixture.write_lock();
    let original = fixture.source();
    fixture.set_source(MATRIX_SOURCE);
    let errors = check_native_package(&fixture.root).unwrap_err();
    assert_eq!(errors[0].code, "CGN004");
    assert!(errors[0].span.is_some());
    fixture.set_source(&original);
    for (sentinel, expected) in [
        ("-301.0", "MatrixDtypeMismatch"),
        ("-302.0", "MatrixShapeMismatch"),
    ] {
        fixture.set_source(
            &original
                .replace(
                    "[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]",
                    &format!("[[{sentinel}]]"),
                )
                .replace("[[7.0, 8.0], [9.0, 10.0], [11.0, 12.0]]", "[[1.0]]"),
        );
        check_native_package(&fixture.root).unwrap();
        let errors = run_native_package(&fixture.root, &[]).unwrap_err();
        assert_eq!(errors[0].code, "CGR024");
        assert!(errors[0].message.contains(expected), "{errors:?}");
        assert!(errors[0].span.is_some());
    }
    fixture.set_source(
        &original
            .replace("[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]", "[[1e20, 1e20]]")
            .replace(
                "[[7.0, 8.0], [9.0, 10.0], [11.0, 12.0]]",
                "[[1e20], [1e20]]",
            ),
    );
    check_native_package(&fixture.root).unwrap();
    let errors = run_native_package(&fixture.root, &[]).unwrap_err();
    assert_eq!(errors[0].code, "CGR024");
}

#[test]
fn float32_source_rejects_missing_export_before_dispatch() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::matrix_f32();
    let mock = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../cgx2-libtorch/facade/mock.c"
    );
    assert!(Command::new("cc")
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-shared",
            "-fPIC",
            "-nostdlib",
            "-DMOCK_MISSING_F32_COPY",
            "-Wl,-soname,libmock.so",
            "-Wl,-rpath,$ORIGIN"
        ])
        .arg(mock)
        .arg("-L")
        .arg(&fixture.root)
        .args(["-Wl,--no-as-needed", "-lcgx2_closure_dep"])
        .arg("-o")
        .arg(fixture.root.join("libmock.so"))
        .status()
        .unwrap()
        .success());
    fixture.lock.bindings[0].sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(fixture.root.join("libmock.so")).unwrap())
    );
    fixture.write_lock();
    let errors = check_native_package(&fixture.root).unwrap_err();
    assert_eq!(errors[0].code, "CGB015");
    assert!(errors[0].span.is_some());
}

#[test]
fn real_pinned_float32_matrix_source_when_available() {
    let Ok(root) = std::env::var("CGX2_LIBTORCH_F32_SOURCE_PACKAGE") else {
        eprintln!("skipped: set CGX2_LIBTORCH_F32_SOURCE_PACKAGE to a reviewed schema-2 float32 source package");
        return;
    };
    let root = PathBuf::from(root);
    check_native_package(&root).unwrap();
    let Value::String(output) = run_native_package(&root, &[]).unwrap().value else {
        panic!("expected matrix string result");
    };
    let values: Vec<f32> = output
        .split(|c: char| {
            !c.is_ascii_digit() && c != '.' && c != '-' && c != 'e' && c != 'E' && c != '+'
        })
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap())
        .collect();
    assert_eq!(values.len(), 4);
    for (actual, expected) in values.iter().zip([58.0, 64.0, 139.0, 154.0]) {
        assert!((actual - expected).abs() <= 1e-5);
    }
}
