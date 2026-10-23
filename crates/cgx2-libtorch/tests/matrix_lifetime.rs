//! Mock ownership tests are separate from the ignored, pinned real-CPU test.
use cgx2_ffi_native::{Library, LibraryDeclaration};
use cgx2_libtorch::{Error, MAX_COPY_ELEMENTS, Torch};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Mock(PathBuf);

impl Mock {
    fn compile(flags: &[&str]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "cgx2-matrix-lifetime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("libmock.so");
        let mut compiler = Command::new("cc");
        compiler.args([
            "-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC",
        ]);
        if std::env::var_os("CGX2_MATRIX_MOCK_ASAN").is_some() {
            compiler.args(["-fsanitize=address", "-fno-omit-frame-pointer", "-g"]);
        }
        let status = compiler
            .args(flags)
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("facade/mock.c"))
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        Self(path)
    }

    fn open(&self) -> Torch {
        // SAFETY: mock.c implements the facade.h v2 signatures.
        unsafe { Torch::open_matrix(&self.0, "2.6.0") }.unwrap()
    }

    fn counter(&self, symbol: &str) -> u64 {
        // SAFETY: mock.c exposes these test-only zero-argument counters.
        unsafe {
            let library = Library::open(
                &self.0,
                &LibraryDeclaration::new("cgx2.libtorch.cpu", "2.6.0"),
            )
            .unwrap();
            let count: unsafe extern "C" fn() -> u64 = library.symbol(symbol).unwrap();
            count()
        }
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        std::fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}

#[test]
fn mock_matrix_repeated_lifecycle_and_exact_fault_classes() {
    let mock = Mock::compile(&[]);
    let torch = mock.open();
    // Includes unit, non-square, and the largest dimensions the mock stores.
    for (cycle, (m, k, n)) in [
        (1, 1, 1),
        (1, 2, 3),
        (2, 1, 2),
        (2, 2, 2),
        (1, 8, 1),
        (8, 1, 1),
    ]
    .into_iter()
    .enumerate()
    {
        let left_values = vec![1.0; m * k];
        let right_values = vec![2.0; k * n];
        let mut left = torch.from_f64_matrix(m, k, &left_values).unwrap();
        let mut right = torch.from_f64_matrix(k, n, &right_values).unwrap();
        let mut product = left.matmul(&right).unwrap();
        assert_eq!(product.shape(), Ok([m, n]));
        let expected = vec![2.0 * k as f64; m * n];
        assert_eq!(product.to_vec_bounded(m * n), Ok(expected.clone()));
        let mut guarded = vec![-99.0; m * n + 2];
        product.copy_into(&mut guarded[1..=m * n]).unwrap();
        assert_eq!(guarded[0], -99.0);
        assert_eq!(guarded[m * n + 1], -99.0);
        assert_eq!(&guarded[1..=m * n], expected);
        let mut short = vec![-77.0; m * n + 1];
        assert_eq!(
            product.copy_into(&mut short[1..m * n]),
            Err(Error::CapacityMismatch {
                expected: m * n,
                provided: m * n - 1,
            })
        );
        assert!(short.iter().all(|value| *value == -77.0));
        if cycle % 2 == 0 {
            product.release().unwrap();
            left.release().unwrap();
            right.release().unwrap();
        } else {
            right.release().unwrap();
            left.release().unwrap();
            product.release().unwrap();
        }
        assert_eq!(product.release(), Err(Error::AlreadyReleased));
        assert_eq!(left.shape(), Err(Error::AlreadyReleased));
        drop((left, right, product));
        assert_eq!(mock.counter("cgx2_mock_live_count"), 0);
        assert_eq!(
            mock.counter("cgx2_mock_release_count"),
            3 * (cycle as u64 + 1)
        );
    }

    assert_eq!(
        torch.from_f64_matrix(0, 1, &[]).err(),
        Some(Error::InvalidMatrixShape)
    );
    assert_eq!(
        torch.from_f64_matrix(usize::MAX, 2, &[]).err(),
        Some(Error::InvalidMatrixShape)
    );
    assert_eq!(
        torch.from_f64_matrix(1, MAX_COPY_ELEMENTS + 1, &[]).err(),
        Some(Error::InvalidMatrixShape)
    );
    assert_eq!(
        torch.from_f64_matrix(2, 2, &[1.0]).err(),
        Some(Error::CapacityMismatch {
            expected: 4,
            provided: 1,
        })
    );
    let a = torch.from_f64_matrix(1, 2, &[1.0, 1.0]).unwrap();
    let b = torch.from_f64_matrix(1, 1, &[2.0]).unwrap();
    assert_eq!(
        a.matmul(&b).err(),
        Some(Error::MatrixDimensionMismatch {
            left_cols: 2,
            right_rows: 1,
        })
    );
    drop((a, b));
    for round in 0..3 {
        assert_eq!(
            torch.from_f64_matrix(1, 1, &[-204.0]).err(),
            Some(Error::Foreign {
                status: 7,
                message: "injected matrix create failure with handle".into(),
            })
        );
        let bad_copy = torch.from_f64_matrix(1, 1, &[-201.0]).unwrap();
        let mut destination = [-99.0, -99.0, -99.0];
        assert_eq!(
            bad_copy.copy_into(&mut destination[1..2]),
            Err(Error::Foreign {
                status: 5,
                message: "injected matrix copy failure".into(),
            })
        );
        assert_eq!(destination, [-99.0; 3]);
        let unit = torch.from_f64_matrix(1, 1, &[1.0]).unwrap();
        let failed = torch.from_f64_matrix(1, 1, &[-206.0]).unwrap();
        assert_eq!(
            failed.matmul(&unit).err(),
            Some(Error::Foreign {
                status: 7,
                message: "injected matmul failure with handle".into(),
            })
        );
        drop((failed, unit, bad_copy));
        assert_eq!(mock.counter("cgx2_mock_live_count"), 0);
        assert_eq!(mock.counter("cgx2_mock_release_count"), 25 + round * 5);
    }
    drop(torch);
}

#[test]
fn mock_matrix_version_and_export_rejection() {
    let wrong = Mock::compile(&["-DMOCK_MATRIX_API_VERSION=3"]);
    assert_eq!(
        // SAFETY: mock.c implements the same C signatures, with a wrong v2 version.
        unsafe { Torch::open_matrix(&wrong.0, "2.6.0") }.err(),
        Some(Error::MatrixApiVersion { reported: 3 })
    );
    let missing = Mock::compile(&["-DMOCK_MISSING_MATRIX_COPY"]);
    assert_eq!(
        // SAFETY: the missing symbol is detected before any matrix call.
        unsafe { Torch::open_matrix(&missing.0, "2.6.0") }.err(),
        Some(Error::Native(cgx2_ffi_native::Error::MissingSymbol(
            "cgx2_torch_matrix_copy_f64".into(),
        )))
    );
}

#[test]
#[ignore = "requires a reviewed schema-2 pinned real LibTorch matrix package"]
fn real_pinned_matrix_element_boundaries() {
    let root = PathBuf::from(std::env::var("CGX2_LIBTORCH_PACKAGE").expect("set package path"));
    assert!(cgx2_bind::check_package(&root).passed());
    // SAFETY: package preflight and closure validation precede every foreign call.
    let torch = unsafe { Torch::open_pinned(&root, "torch_cpu") }.unwrap();
    assert_eq!(
        torch.from_f64_matrix(0, 1, &[]).err(),
        Some(Error::InvalidMatrixShape)
    );
    assert_eq!(
        torch.from_f64_matrix(usize::MAX, 2, &[]).err(),
        Some(Error::InvalidMatrixShape)
    );
    assert_eq!(
        torch.from_f64_matrix(1, MAX_COPY_ELEMENTS + 1, &[]).err(),
        Some(Error::InvalidMatrixShape)
    );
    let mut large = torch
        .from_f64_matrix(MAX_COPY_ELEMENTS, 1, &vec![1.0; MAX_COPY_ELEMENTS])
        .unwrap();
    let mut scalar = torch.from_f64_matrix(1, 1, &[2.0]).unwrap();
    let mut product = large.matmul(&scalar).unwrap();
    assert_eq!(product.shape(), Ok([MAX_COPY_ELEMENTS, 1]));
    let mut copied = vec![0.0; MAX_COPY_ELEMENTS];
    product.copy_into(&mut copied).unwrap();
    assert!(copied.iter().all(|value| *value == 2.0));
    assert_eq!(
        product.copy_into(&mut copied[..MAX_COPY_ELEMENTS - 1]),
        Err(Error::CapacityMismatch {
            expected: MAX_COPY_ELEMENTS,
            provided: MAX_COPY_ELEMENTS - 1,
        })
    );
    scalar.release().unwrap();
    product.release().unwrap();
    large.release().unwrap();
    assert_eq!(large.release(), Err(Error::AlreadyReleased));
    assert!(
        !std::fs::read_to_string("/proc/self/maps")
            .unwrap()
            .contains("libpython")
    );
}
