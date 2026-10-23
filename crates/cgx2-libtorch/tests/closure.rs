#![cfg(target_os = "linux")]

use cgx2_bind::{
    Binding, BindingLock, ClosureLock, LibraryPin, check_package, libtorch_facade_symbols,
    render_lock, verify_libtorch_closure,
};
use cgx2_libtorch::{Error, PinnedError, Torch};
use sha2::{Digest, Sha256};
use std::ffi::{CString, c_char, c_int, c_void};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
static LOADER_TEST_LOCK: Mutex<()> = Mutex::new(());
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
    fn dlclose(handle: *mut c_void) -> c_int;
}

#[test]
fn closure_accepts_and_rejects_changed_version_and_loader_substitution() {
    let _loader_guard = LOADER_TEST_LOCK.lock().unwrap();
    let good = Fixture::new("2.6.0");
    assert!(check_package(&good.root).passed());
    let torch = unsafe { Torch::open_pinned(&good.root, "torch_cpu") }.unwrap();
    let tensor = torch.from_f64(&[3.0]).unwrap();
    assert_eq!(tensor.shape().unwrap(), [1]);
    assert_eq!(tensor.to_vec().unwrap(), vec![3.0]);
    // An arbitrary executable filename without a SONAME is benign while its
    // mapped inode is inspectable. Replacing the pathname must fail closed.
    let arbitrary = good.root.join("arbitrary_executable");
    let dep_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/closure/dep.c");
    compile(&["-shared", "-fPIC", "-nostdlib"], &dep_source, &arbitrary);
    let arbitrary_name = CString::new(arbitrary.to_str().unwrap()).unwrap();
    let arbitrary_handle = unsafe { dlopen(arbitrary_name.as_ptr(), 2) };
    assert!(!arbitrary_handle.is_null());
    let closure = verify_libtorch_closure(&good.root, &good.binding).unwrap();
    assert!(closure.verify_preloaded().is_ok());
    let original = good.root.join("arbitrary_original");
    fs::copy(&arbitrary, &original).unwrap();
    fs::remove_file(&arbitrary).unwrap();
    fs::copy(&original, &arbitrary).unwrap();
    assert!(matches!(
        closure.verify_preloaded(),
        Err(cgx2_bind::ClosureError::UnverifiedLoader(message))
            if message.contains("executable mapping was deleted")
    ));
    assert_eq!(unsafe { dlclose(arbitrary_handle) }, 0);
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
    let evil_handle = unsafe { dlopen(evil_path.as_ptr(), 2) };
    assert!(!evil_handle.is_null());
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
    assert_eq!(unsafe { dlclose(evil_handle) }, 0);
}

#[test]
fn changed_dependency_after_preflight_is_rejected_before_loading() {
    let fixture = Fixture::new("2.6.0");
    let closure = verify_libtorch_closure(&fixture.root, &fixture.binding).unwrap();
    fs::write(fixture.root.join(DEP), b"changed after preflight").unwrap();
    assert!(closure.verify_preloaded().is_err());
}

#[test]
fn mock_package_preparation_is_repeatable_and_rejects_ambiguous_needed() {
    let _loader_guard = LOADER_TEST_LOCK.lock().unwrap();
    let fixture = Fixture::new("2.6.0");
    let package = fixture.root.join("copied-package");
    fs::create_dir(fixture.root.join("lib")).unwrap();
    let compiler = fs::canonicalize("/usr/bin/cc").unwrap();
    fs::write(
        fixture.root.join("CMakeCache.txt"),
        format!(
            "CGX2_EXPECT_TORCH_VERSION:UNINITIALIZED=2.6.0\nCMAKE_CXX_COMPILER:FILEPATH={}\nCMAKE_PREFIX_PATH:UNINITIALIZED={}\n",
            compiler.display(),
            fixture.root.display()
        ),
    )
    .unwrap();
    fs::write(
        fixture.root.join("cgx2-torch-cxx-flags.txt"),
        "-D_GLIBCXX_USE_CXX11_ABI=0",
    )
    .unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("prepare_pinned.py");
    let run = |package: &Path| {
        Command::new("python3")
            .arg(&script)
            .args([
                "--facade",
                fixture.root.join(FACADE).to_str().unwrap(),
                "--libtorch-root",
                fixture.root.to_str().unwrap(),
                "--build-dir",
                fixture.root.to_str().unwrap(),
                "--package",
                package.to_str().unwrap(),
                "--version",
                "2.6.0",
                "--compiler",
                compiler.to_str().unwrap(),
                "--cxx11-abi",
                "0",
                "--torch-cxx-flags=-D_GLIBCXX_USE_CXX11_ABI=0",
                "--build-command",
                "cc -shared mock fixture",
                "--lib-dir",
                fixture.root.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let first = run(&package);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let locks = [
        "cgx2-bind.lock.json",
        "cgx2-libtorch-closure.lock.json",
        "cgx2-libtorch-evidence.json",
    ]
    .map(|name| fs::read(package.join(name)).unwrap());
    let second = run(&package);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    for (name, bytes) in [
        "cgx2-bind.lock.json",
        "cgx2-libtorch-closure.lock.json",
        "cgx2-libtorch-evidence.json",
    ]
    .into_iter()
    .zip(locks)
    {
        assert_eq!(fs::read(package.join(name)).unwrap(), bytes);
    }
    let closure: ClosureLock =
        serde_json::from_slice(&fs::read(package.join("cgx2-libtorch-closure.lock.json")).unwrap())
            .unwrap();
    assert_eq!(closure.libraries.len(), 2);
    for pin in &closure.libraries {
        assert_eq!(pin.path.parent(), Some(package.as_path()));
        assert_eq!(pin.sha256, hash(&pin.path));
        assert!(!pin.path.is_symlink());
        assert_eq!(pin.soname.as_deref(), Some(pin.name.as_str()));
        assert_ne!(
            fs::metadata(&pin.path).unwrap().ino(),
            fs::metadata(fixture.root.join(&pin.name)).unwrap().ino()
        );
    }
    let binding: BindingLock =
        serde_json::from_slice(&fs::read(package.join("cgx2-bind.lock.json")).unwrap()).unwrap();
    assert_eq!(binding.schema_version, 2);
    assert_eq!(binding.bindings[0].version, "2.6.0");
    assert_eq!(binding.bindings[0].abi, "cgx2-libtorch-facade-v1");
    let evidence: serde_json::Value =
        serde_json::from_slice(&fs::read(package.join("cgx2-libtorch-evidence.json")).unwrap())
            .unwrap();
    assert_eq!(evidence["torch_version"], "2.6.0");
    assert_eq!(evidence["cxx11_abi"], 0);
    assert_eq!(evidence["torch_cxx_flags"], "-D_GLIBCXX_USE_CXX11_ABI=0");
    for library in evidence["libraries"].as_array().unwrap() {
        assert_eq!(library["packaged"], true);
        assert!(
            library["source_path"]
                .as_str()
                .unwrap()
                .starts_with(fixture.root.to_str().unwrap())
        );
        assert!(
            library["path"]
                .as_str()
                .unwrap()
                .starts_with(package.to_str().unwrap())
        );
    }
    assert!(check_package(&package).passed());
    let torch = unsafe { Torch::open_pinned(&package, "torch_cpu") }.unwrap();
    let mut tensor = torch.from_f64(&[3.0]).unwrap();
    assert_eq!(tensor.shape().unwrap(), [1]);
    assert_eq!(tensor.to_vec_bounded(1).unwrap(), [3.0]);
    tensor.release().unwrap();
    assert_eq!(tensor.release(), Err(Error::AlreadyReleased));
    assert!(matches!(
        torch.from_f64(&[1.0, 2.0]),
        Err(Error::Foreign { status: 1, .. })
    ));
    drop(torch);

    let verified = verify_libtorch_closure(&package, &fixture.binding).unwrap();
    let replacement = package.join("replacement-dependency.so");
    fs::copy(package.join(DEP), &replacement).unwrap();
    fs::rename(&replacement, package.join(DEP)).unwrap();
    assert!(
        verified.verify_preloaded().is_err(),
        "changed inode must fail closed"
    );
    assert!(
        check_package(&package).passed(),
        "same bytes still match the lock"
    );
    fs::write(package.join(DEP), b"changed copied dependency bytes").unwrap();
    assert!(
        !check_package(&package).passed(),
        "changed bytes must fail preflight"
    );
    assert!(
        run(&package).status.success(),
        "preparation restores pinned bytes"
    );
    let source_dep = fixture.root.join(DEP);
    let hidden_dep = fixture.root.join("hidden-dependency.so");
    fs::rename(&source_dep, &hidden_dep).unwrap();
    let missing = run(&package);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("missing or ambiguous DT_NEEDED"));
    fs::rename(&hidden_dep, &source_dep).unwrap();
    let cache = fixture.root.join("CMakeCache.txt");
    let valid_cache = fs::read(&cache).unwrap();
    fs::write(
        &cache,
        String::from_utf8(valid_cache.clone())
            .unwrap()
            .replace("2.6.0", "2.7.0"),
    )
    .unwrap();
    let version = run(&package);
    assert!(!version.status.success());
    assert!(String::from_utf8_lossy(&version.stderr).contains("version"));
    fs::write(&cache, valid_cache).unwrap();
    let flags = fixture.root.join("cgx2-torch-cxx-flags.txt");
    fs::write(&flags, "-D_GLIBCXX_USE_CXX11_ABI=1").unwrap();
    let abi = run(&package);
    assert!(!abi.status.success());
    assert!(String::from_utf8_lossy(&abi.stderr).contains("TORCH_CXX_FLAGS"));
    fs::write(&flags, "-D_GLIBCXX_USE_CXX11_ABI=0").unwrap();

    fs::copy(fixture.root.join(DEP), fixture.root.join("lib").join(DEP)).unwrap();
    let ambiguous = run(&package);
    assert!(!ambiguous.status.success());
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("ambiguous DT_NEEDED"));

    fs::remove_file(fixture.root.join("lib").join(DEP)).unwrap();
    let substituted = fixture.root.join("substituted");
    fs::create_dir(&substituted).unwrap();
    std::os::unix::fs::symlink(fixture.root.join(FACADE), substituted.join(FACADE)).unwrap();
    let symlink = run(&substituted);
    assert!(!symlink.status.success());
    assert!(String::from_utf8_lossy(&symlink.stderr).contains("unsafe package library target"));

    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/closure/facade.c");
    let unsafe_facade = fixture.root.join("unsafe-facade.so");
    compile(
        &[
            "-shared",
            "-fPIC",
            "-nostdlib",
            "-Wl,-soname,libcgx2_libtorch_facade.so",
            "-Wl,-rpath,/tmp",
            "-L",
            fixture.root.to_str().unwrap(),
            "-Wl,--no-as-needed",
            "-lcgx2_closure_dep",
        ],
        &source,
        &unsafe_facade,
    );
    fs::rename(&unsafe_facade, fixture.root.join(FACADE)).unwrap();
    let unsafe_search = run(&package);
    assert!(!unsafe_search.status.success());
    assert!(String::from_utf8_lossy(&unsafe_search.stderr).contains("RUNPATH"));
}

#[test]
#[ignore = "requires a reviewed schema-2 package and exact compatible LibTorch installation"]
fn real_pinned_libtorch_when_installed() {
    let root =
        PathBuf::from(std::env::var("CGX2_LIBTORCH_PACKAGE").expect("set CGX2_LIBTORCH_PACKAGE"));
    assert!(check_package(&root).passed());
    let torch = unsafe { Torch::open_pinned(&root, "torch_cpu") }.unwrap();
    let mut x = torch.from_f64(&[1.0, 2.0, 3.0]).unwrap();
    let mut y = torch.from_f64(&[4.0, 5.0, 6.0]).unwrap();
    let mut sum = x.add(&y).unwrap();
    assert_eq!(sum.shape().unwrap(), [3]);
    assert_eq!(sum.to_vec_bounded(3).unwrap(), vec![5.0, 7.0, 9.0]);
    sum.release().unwrap();
    x.release().unwrap();
    y.release().unwrap();
    assert_eq!(sum.release(), Err(Error::AlreadyReleased));
    assert!(
        !fs::read_to_string("/proc/self/maps")
            .unwrap()
            .contains("libpython")
    );
}
