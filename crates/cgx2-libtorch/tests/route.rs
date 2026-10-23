use cgx2_ffi_native::{Library, LibraryDeclaration};
use cgx2_libtorch::{Error, MAX_ERROR_BYTES, Torch};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Mock(PathBuf);
impl Mock {
    fn compile(flags: &[&str]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "cgx2-libtorch-mock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("libmock.so");
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let status = Command::new("cc")
            .args([
                "-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC",
            ])
            .args(flags)
            .arg(root.join("facade/mock.c"))
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        Self(path)
    }

    fn open(&self) -> Torch {
        // SAFETY: mock.c implements the exact facade.h C signatures.
        unsafe { Torch::open(&self.0, "2.6.0") }.unwrap()
    }

    fn counter(&self, name: &str) -> u64 {
        // SAFETY: mock.c defines this test-only zero-argument counter signature.
        unsafe {
            let library = Library::open(
                &self.0,
                &LibraryDeclaration::new("cgx2.libtorch.cpu", "2.6.0"),
            )
            .unwrap();
            let function: unsafe extern "C" fn() -> u64 = library.symbol(name).unwrap();
            function()
        }
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        std::fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}

#[test]
fn mock_facade_cpu_contract_and_raii() {
    let mock = Mock::compile(&[]);
    let torch = mock.open();
    let mut left = torch.from_f64(&[1.0, 2.0]).unwrap();
    let right = torch.from_f64(&[3.0, 4.0]).unwrap();
    let sum = left.add(&right).unwrap();
    assert_eq!(sum.shape().unwrap(), [2]);
    assert_eq!(sum.dtype(), "float64");
    assert_eq!(sum.device(), "cpu");
    assert_eq!(sum.to_vec().unwrap(), [4.0, 6.0]);
    assert_eq!(sum.to_vec_bounded(2).unwrap(), [4.0, 6.0]);
    assert!(matches!(
        sum.to_vec_bounded(1),
        Err(Error::LengthLimitExceeded {
            reported: 2,
            maximum: 1
        })
    ));
    assert_eq!(mock.counter("cgx2_mock_live_count"), 3);
    left.release().unwrap();
    assert_eq!(left.release(), Err(Error::AlreadyReleased));
    assert_eq!(mock.counter("cgx2_mock_release_count"), 1);
    drop((left, right, sum));
    assert_eq!(mock.counter("cgx2_mock_live_count"), 0);
    assert_eq!(mock.counter("cgx2_mock_release_count"), 3);
}

#[test]
fn mock_facade_rejects_wrong_metadata_and_missing_export() {
    for (flag, expected) in [
        (
            "-DMOCK_VERSION=\"9.0.0\"",
            cgx2_ffi_native::Error::VersionMismatch,
        ),
        ("-DMOCK_ABI=2", cgx2_ffi_native::Error::AbiMismatch),
        (
            "-DMOCK_IDENTITY=\"wrong\"",
            cgx2_ffi_native::Error::IdentityMismatch,
        ),
    ] {
        let mock = Mock::compile(&[flag]);
        // SAFETY: mock.c retains facade.h signatures even when metadata differs.
        assert_eq!(
            unsafe { Torch::open(&mock.0, "2.6.0") }.err(),
            Some(Error::Native(expected))
        );
    }
    let mock = Mock::compile(&["-DMOCK_MISSING_COPY"]);
    // SAFETY: mock.c intentionally omits one export; no tensor call is made.
    assert_eq!(
        unsafe { Torch::open(&mock.0, "2.6.0") }.err(),
        Some(Error::Native(cgx2_ffi_native::Error::MissingSymbol(
            "cgx2_torch_copy_f64".into()
        )))
    );
}

#[test]
fn mock_facade_rejects_bad_shape_profile_and_cross_adapter_handle() {
    let mock = Mock::compile(&[]);
    let first = mock.open();
    let second = mock.open();
    let a = first.from_f64(&[1.0]).unwrap();
    let b = second.from_f64(&[2.0]).unwrap();
    assert_eq!(a.add(&b).err(), Some(Error::DifferentRuntime));
    let short = first.from_f64(&[1.0, 2.0]).unwrap();
    assert!(matches!(
        a.add(&short),
        Err(Error::Foreign { status: 2, .. })
    ));
    for (marker, message) in [
        (-103.0, "invalid dtype"),
        (-104.0, "invalid device"),
        (-105.0, "invalid shape"),
    ] {
        let bad = first.from_f64(&[marker]).unwrap();
        assert_eq!(
            bad.shape(),
            Err(Error::Foreign {
                status: 3,
                message: message.into(),
            })
        );
    }
    let bad_copy = first.from_f64(&[-107.0]).unwrap();
    assert!(matches!(
        bad_copy.to_vec(),
        Err(Error::Foreign { status: 5, .. })
    ));
    let bad_length = first.from_f64(&[-106.0]).unwrap();
    assert_eq!(
        bad_length.to_vec(),
        Err(Error::ShapeMismatch {
            expected: 1,
            reported: 2
        })
    );
    let oversized = first.from_f64(&[-108.0]).unwrap();
    assert!(matches!(
        oversized.to_vec_bounded(8),
        Err(Error::LengthLimitExceeded { maximum: 8, .. })
    ));
    drop((a, b, short, bad_copy, bad_length, oversized));
    assert_eq!(mock.counter("cgx2_mock_live_count"), 0);
}

#[test]
fn mock_facade_failed_create_cleans_handle_and_never_publishes_values() {
    let mock = Mock::compile(&[]);
    let torch = mock.open();
    assert!(matches!(
        torch.from_f64(&[-101.0]),
        Err(Error::Foreign { status: 7, .. })
    ));
    assert_eq!(mock.counter("cgx2_mock_live_count"), 0);
    assert_eq!(mock.counter("cgx2_mock_release_count"), 1);
    assert!(matches!(torch.from_f64(&[-102.0]), Err(Error::NullHandle)));
    match torch.from_f64(&[-113.0]) {
        Err(Error::Foreign { status: 7, message }) => {
            assert_eq!(message.len(), MAX_ERROR_BYTES + " [truncated]".len());
            assert!(message.starts_with('x'));
            assert!(message.ends_with(" [truncated]"));
        }
        _ => panic!("expected bounded foreign error"),
    }
}

#[test]
fn aliased_output_never_releases_an_existing_handle() {
    let mock = Mock::compile(&[]);
    let torch = mock.open();
    let owner = torch.from_f64(&[4.0]).unwrap();
    assert!(matches!(
        torch.from_f64(&[-109.0]),
        Err(Error::Foreign { status: -1, .. })
    ));
    assert!(matches!(
        torch.from_f64(&[-110.0]),
        Err(Error::Foreign { status: 7, .. })
    ));
    assert_eq!(owner.to_vec_bounded(1).unwrap(), [4.0]);
    assert_eq!(mock.counter("cgx2_mock_live_count"), 1);
    assert_eq!(mock.counter("cgx2_mock_release_count"), 0);
    let retained = torch.clone();
    drop(torch);
    drop(owner);
    assert_eq!(mock.counter("cgx2_mock_live_count"), 0);
    assert_eq!(mock.counter("cgx2_mock_release_count"), 1);
    drop(retained);
}

#[test]
fn aliased_add_and_release_error_keep_single_owner() {
    let mock = Mock::compile(&[]);
    let torch = mock.open();
    let owner = torch.from_f64(&[-112.0]).unwrap();
    assert!(matches!(
        owner.add(&owner),
        Err(Error::Foreign { status: -1, .. })
    ));
    assert_eq!(owner.to_vec_bounded(1).unwrap(), [-112.0]);
    let mut uncertain_release = torch.from_f64(&[-111.0]).unwrap();
    assert!(matches!(
        uncertain_release.release(),
        Err(Error::Foreign { status: 6, .. })
    ));
    assert_eq!(uncertain_release.release(), Err(Error::AlreadyReleased));
    drop(uncertain_release);
    assert_eq!(mock.counter("cgx2_mock_live_count"), 1);
    assert_eq!(mock.counter("cgx2_mock_release_count"), 1);
    drop(owner);
    assert_eq!(mock.counter("cgx2_mock_live_count"), 0);
    assert_eq!(mock.counter("cgx2_mock_release_count"), 2);
}

#[test]
fn mock_facade_c_copy_checks_capacity_before_writing() {
    let mock = Mock::compile(&[]);
    // SAFETY: mock.c declares these exact C signatures; the handle is live
    // through copy and released exactly once after the capacity assertions.
    unsafe {
        let library = Library::open(
            &mock.0,
            &LibraryDeclaration::new("cgx2.libtorch.cpu", "2.6.0"),
        )
        .unwrap();
        let create: unsafe extern "C" fn(*const f64, u64, *mut *mut std::ffi::c_void) -> i32 =
            library.symbol("cgx2_torch_f64_cpu").unwrap();
        let copy: unsafe extern "C" fn(*const std::ffi::c_void, *mut f64, u64) -> i32 =
            library.symbol("cgx2_torch_copy_f64").unwrap();
        let release: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32 =
            library.symbol("cgx2_torch_release").unwrap();
        let mut raw = std::ptr::null_mut();
        assert_eq!(create([1.0, 2.0].as_ptr(), 2, &mut raw), 0);
        let mut guarded = [91.0, 92.0, 93.0];
        assert_eq!(copy(raw, guarded.as_mut_ptr().add(1), 1), 4);
        assert_eq!(guarded, [91.0, 92.0, 93.0]);
        assert_eq!(copy(raw, guarded.as_mut_ptr(), 3), 4);
        assert_eq!(guarded, [91.0, 92.0, 93.0]);
        assert_eq!(release(raw), 0);
        assert_eq!(release(raw), 6);
    }
}

#[test]
fn static_route_contains_no_python_link() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in [
        "Cargo.toml",
        "facade/CMakeLists.txt",
        "facade/facade.cpp",
        "facade/facade.h",
    ] {
        let source = std::fs::read_to_string(root.join(file))
            .unwrap()
            .to_lowercase();
        for forbidden in ["pyo3", "python.h", "libtorch_python", "libpython"] {
            assert!(!source.contains(forbidden), "{file} references {forbidden}");
        }
    }
    assert!(
        Command::new("cc")
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
                "-x",
                "c"
            ])
            .arg(root.join("facade/facade.h"))
            .status()
            .unwrap()
            .success()
    );
}

#[test]
#[ignore = "requires CGX2_LIBTORCH_FACADE and CGX2_LIBTORCH_VERSION for a real LibTorch artifact"]
fn real_libtorch_cpu_when_installed() {
    let path = std::env::var("CGX2_LIBTORCH_FACADE").expect("set CGX2_LIBTORCH_FACADE");
    let version = std::env::var("CGX2_LIBTORCH_VERSION").expect("set CGX2_LIBTORCH_VERSION");
    let dependencies = Command::new("ldd").arg(&path).output().expect("ldd");
    assert!(dependencies.status.success());
    assert!(!String::from_utf8_lossy(&dependencies.stdout).contains("libpython"));
    // SAFETY: opt-in test requires a reviewed facade built from facade.h.
    let torch = unsafe { Torch::open(Path::new(&path), &version) }.unwrap();
    let mut x = torch.from_f64(&[1.0, 2.0, 3.0]).unwrap();
    let y = torch.from_f64(&[4.0, 5.0, 6.0]).unwrap();
    let sum = x.add(&y).unwrap();
    assert_eq!(sum.shape().unwrap(), [3]);
    assert_eq!(sum.to_vec().unwrap(), [5.0, 7.0, 9.0]);
    x.release().unwrap();
    assert_eq!(x.release(), Err(Error::AlreadyReleased));
    assert_eq!(
        unsafe { Torch::open(Path::new(&path), "deliberately-wrong-version") }.err(),
        Some(Error::Native(cgx2_ffi_native::Error::VersionMismatch))
    );
}
