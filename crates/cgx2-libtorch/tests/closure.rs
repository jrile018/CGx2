#![cfg(target_os = "linux")]

use cgx2_bind::{
    Binding, BindingLock, ClosureLock, LibraryPin, check_package, libtorch_facade_symbols,
    render_lock, verify_libtorch_closure,
};
use cgx2_libtorch::{Error, PinnedError, Torch};
use sha2::{Digest, Sha256};
use std::ffi::{CString, c_char, c_int, c_void};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const DEP: &str = "libcgx2_closure_dep.so";
const FACADE: &str = "libcgx2_libtorch_facade.so";

struct Fixture {
    root: PathBuf,
    binding: Binding,
}

impl Fixture {
    fn new(version: &str) -> Self {
        Self::with_constructor(version, false)
    }

    fn with_constructor(version: &str, trap: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-closure-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("cgx2.toml"), "schema_version = 2\nname = \"closure_mock\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\nbinding_lock = \"cgx2-bind.lock.json\"\n").unwrap();
        fs::write(root.join("src/main.cg"), "module closure_mock\n").unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/closure");
        compile(
            &[
                "-shared",
                "-fPIC",
                "-nostdlib",
                "-Wl,-soname,libcgx2_closure_dep.so",
            ],
            &source.join("dep.c"),
            &root.join(DEP),
        );
        compile(
            &[
                "-shared",
                "-fPIC",
                "-nostdlib",
                &format!("-DMOCK_VERSION=\"{version}\""),
                if trap {
                    "-DMOCK_TRAP_CONSTRUCTOR"
                } else {
                    "-DMOCK_NO_TRAP"
                },
                "-Wl,-soname,libcgx2_libtorch_facade.so",
                "-Wl,-rpath,$ORIGIN",
                "-L",
                root.to_str().unwrap(),
                "-Wl,--no-as-needed",
                "-lcgx2_closure_dep",
            ],
            &source.join("facade.c"),
            &root.join(FACADE),
        );
        let binding = Binding {
            id: "torch_cpu".into(),
            route: "libtorch".into(),
            library: "cgx2.libtorch.cpu".into(),
            version: "2.6.0".into(),
            artifact: FACADE.into(),
            sha256: hash(&root.join(FACADE)),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            abi: "cgx2-libtorch-facade-v1".into(),
            calling_convention: "c".into(),
            layout: "tensor-f64-cpu-v1".into(),
            symbols: libtorch_facade_symbols(),
        };
        let fixture = Self { root, binding };
        fixture.write_locks();
        fixture
    }

    fn write_locks(&self) {
        let lock = BindingLock {
            schema_version: 2,
            package: "closure_mock".into(),
            bindings: vec![self.binding.clone()],
        };
        fs::write(
            self.root.join("cgx2-bind.lock.json"),
            render_lock(&lock).unwrap(),
        )
        .unwrap();
        let libraries = [("facade", FACADE), ("dependency", DEP)]
            .into_iter()
            .map(|(role, name)| LibraryPin {
                role: role.into(),
                name: name.into(),
                path: self.root.join(name).canonicalize().unwrap(),
                sha256: hash(&self.root.join(name)),
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
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn compile(flags: &[&str], source: &Path, output: &Path) {
    assert!(
        Command::new("cc")
            .args(flags)
            .arg(source)
            .arg("-o")
            .arg(output)
            .status()
            .unwrap()
            .success()
    );
}

fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
}

#[test]
fn closure_accepts_and_rejects_changed_version_and_loader_substitution() {
    let good = Fixture::new("2.6.0");
    assert!(check_package(&good.root).passed());
    let torch = unsafe { Torch::open_pinned(&good.root, "torch_cpu") }.unwrap();
    let tensor = torch.from_f64(&[3.0]).unwrap();
    assert_eq!(tensor.shape().unwrap(), [1]);
    assert_eq!(tensor.to_vec().unwrap(), vec![3.0]);
    for line in fs::read_to_string("/proc/self/maps").unwrap().lines() {
        if line.contains(&good.root.to_string_lossy().to_string())
            && (line.contains(FACADE) || line.contains(DEP))
        {
            eprintln!("observed pinned module: {line}");
        }
    }
    drop(tensor);
    fs::rename(
        good.root.join(DEP),
        good.root.join("previous-dependency.so"),
    )
    .unwrap();
    fs::copy(
        good.root.join("previous-dependency.so"),
        good.root.join(DEP),
    )
    .unwrap();
    assert!(matches!(
        torch.from_f64(&[1.0]),
        Err(Error::UnverifiedLoader(_))
    ));
    drop(torch);
    drop(good);

    let altered = Fixture::new("2.6.0");
    fs::write(altered.root.join(DEP), b"altered dependency").unwrap();
    assert!(
        check_package(&altered.root)
            .diagnostics
            .iter()
            .any(|d| d.code == "CGB019" && d.message.contains("SHA-256 mismatch"))
    );
    drop(altered);

    let mut additional_needed = Fixture::new("2.6.0");
    let extra_name = "libcgx2_closure_extra_needed.so";
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/closure");
    compile(
        &[
            "-shared",
            "-fPIC",
            "-nostdlib",
            "-Wl,-soname,libcgx2_closure_extra_needed.so",
        ],
        &source.join("dep.c"),
        &additional_needed.root.join(extra_name),
    );
    compile(
        &[
            "-shared",
            "-fPIC",
            "-nostdlib",
            "-Wl,-soname,libcgx2_libtorch_facade.so",
            "-Wl,-rpath,$ORIGIN",
            "-L",
            additional_needed.root.to_str().unwrap(),
            "-Wl,--no-as-needed",
            "-lcgx2_closure_dep",
            "-lcgx2_closure_extra_needed",
        ],
        &source.join("facade.c"),
        &additional_needed.root.join(FACADE),
    );
    additional_needed.binding.sha256 = hash(&additional_needed.root.join(FACADE));
    additional_needed.write_locks();
    assert!(
        check_package(&additional_needed.root)
            .diagnostics
            .iter()
            .any(|d| d.code == "CGB019" && d.message.contains("missing DT_NEEDED"))
    );
    drop(additional_needed);

    let mut bad_search = Fixture::new("2.6.0");
    compile(
        &[
            "-shared",
            "-fPIC",
            "-nostdlib",
            "-Wl,-soname,libcgx2_libtorch_facade.so",
            "-Wl,-rpath,/tmp",
            "-L",
            bad_search.root.to_str().unwrap(),
            "-Wl,--no-as-needed",
            "-lcgx2_closure_dep",
        ],
        &source.join("facade.c"),
        &bad_search.root.join(FACADE),
    );
    bad_search.binding.sha256 = hash(&bad_search.root.join(FACADE));
    bad_search.write_locks();
    assert!(
        check_package(&bad_search.root)
            .diagnostics
            .iter()
            .any(|d| d.code == "CGB019" && d.message.contains("DT_RUNPATH"))
    );
    drop(bad_search);

    let missing = Fixture::new("2.6.0");
    let closure_path = missing.root.join("cgx2-libtorch-closure.lock.json");
    let mut closure: ClosureLock =
        serde_json::from_slice(&fs::read(&closure_path).unwrap()).unwrap();
    closure.libraries.pop();
    fs::write(&closure_path, serde_json::to_vec(&closure).unwrap()).unwrap();
    assert!(
        check_package(&missing.root)
            .diagnostics
            .iter()
            .any(|d| d.code == "CGB019" && d.message.contains("missing DT_NEEDED"))
    );
    drop(missing);

    let extra = Fixture::new("2.6.0");
    let extra_name = "libcgx2_closure_extra.so";
    let extra_path = extra.root.join(extra_name);
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/closure/dep.c");
    compile(
        &[
            "-shared",
            "-fPIC",
            "-nostdlib",
            "-Wl,-soname,libcgx2_closure_extra.so",
        ],
        &source,
        &extra_path,
    );
    let closure_path = extra.root.join("cgx2-libtorch-closure.lock.json");
    let mut closure: ClosureLock =
        serde_json::from_slice(&fs::read(&closure_path).unwrap()).unwrap();
    closure.libraries.push(LibraryPin {
        role: "dependency".into(),
        name: extra_name.into(),
        path: extra_path.canonicalize().unwrap(),
        sha256: hash(&extra_path),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        soname: Some(extra_name.into()),
    });
    fs::write(&closure_path, serde_json::to_vec(&closure).unwrap()).unwrap();
    assert!(
        check_package(&extra.root)
            .diagnostics
            .iter()
            .any(|d| d.code == "CGB019" && d.message.contains("unreachable"))
    );
    drop(extra);

    let symlinked = Fixture::new("2.6.0");
    let alias = symlinked.root.join("alias.so");
    std::os::unix::fs::symlink(symlinked.root.join(DEP), &alias).unwrap();
    let closure_path = symlinked.root.join("cgx2-libtorch-closure.lock.json");
    let mut closure: ClosureLock =
        serde_json::from_slice(&fs::read(&closure_path).unwrap()).unwrap();
    closure.libraries[1].path = alias;
    fs::write(&closure_path, serde_json::to_vec(&closure).unwrap()).unwrap();
    assert!(
        check_package(&symlinked.root)
            .diagnostics
            .iter()
            .any(|d| d.code == "CGB019" && d.message.contains("symlink"))
    );
    drop(symlinked);

    let wrong_version = Fixture::new("2.5.0");
    assert!(check_package(&wrong_version.root).passed());
    assert!(matches!(
        unsafe { Torch::open_pinned(&wrong_version.root, "torch_cpu") },
        Err(PinnedError::Adapter(_))
    ));
    drop(wrong_version);

    let substituted = Fixture::new("2.6.0");
    let evil = substituted.root.join("evil");
    fs::create_dir(&evil).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/closure/dep.c");
    compile(
        &[
            "-shared",
            "-fPIC",
            "-nostdlib",
            "-DSUBSTITUTE",
            "-Wl,-soname,libcgx2_closure_dep.so",
        ],
        &source,
        &evil.join("hidden_dependency"),
    );
    let evil_path = CString::new(evil.join("hidden_dependency").to_str().unwrap()).unwrap();
    assert!(!unsafe { dlopen(evil_path.as_ptr(), 2) }.is_null());
    assert!(check_package(&substituted.root).passed());
    assert!(matches!(
        unsafe { Torch::open_pinned(&substituted.root, "torch_cpu") },
        Err(PinnedError::Closure(_))
    ));

    // A preloaded conflicting SONAME must be rejected before this facade's
    // constructor can execute. Executing it would trap the test process.
    let trap = Fixture::with_constructor("2.6.0", true);
    assert!(check_package(&trap.root).passed());
    assert!(matches!(
        unsafe { Torch::open_pinned(&trap.root, "torch_cpu") },
        Err(PinnedError::Closure(_))
    ));
}

#[test]
fn changed_dependency_after_preflight_is_rejected_before_loading() {
    let fixture = Fixture::new("2.6.0");
    let closure = verify_libtorch_closure(&fixture.root, &fixture.binding).unwrap();
    fs::write(fixture.root.join(DEP), b"changed after preflight").unwrap();
    assert!(closure.verify_preloaded().is_err());
}

#[test]
#[ignore = "requires a reviewed schema-2 package and exact compatible LibTorch installation"]
fn real_pinned_libtorch_when_installed() {
    let root =
        PathBuf::from(std::env::var("CGX2_LIBTORCH_PACKAGE").expect("set CGX2_LIBTORCH_PACKAGE"));
    assert!(check_package(&root).passed());
    let torch = unsafe { Torch::open_pinned(&root, "torch_cpu") }.unwrap();
    let x = torch.from_f64(&[1.0, 2.0, 3.0]).unwrap();
    let y = torch.from_f64(&[4.0, 5.0, 6.0]).unwrap();
    let sum = x.add(&y).unwrap();
    assert_eq!(sum.shape().unwrap(), [3]);
    assert_eq!(sum.to_vec().unwrap(), vec![5.0, 7.0, 9.0]);
    assert!(
        !fs::read_to_string("/proc/self/maps")
            .unwrap()
            .contains("libpython")
    );
}
