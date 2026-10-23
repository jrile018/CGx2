//! Mock facade checks; the ignored pinned test below requires a real CPU package.
use cgx2_libtorch::{Error, F32_DTYPE_CODE, MAX_COPY_ELEMENTS, Torch};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Mock(PathBuf);
impl Mock {
    fn compile(flags: &[&str]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "cgx2-f32-mock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("libmock.so");
        assert!(
            Command::new("cc")
                .args([
                    "-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC"
                ])
                .args(flags)
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("facade/mock.c"))
                .arg("-o")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        Self(path)
    }
    fn open(&self) -> Torch {
        // SAFETY: mock.c has the exact reviewed test ABI, including v1/v2/v3.
        unsafe { Torch::open_matrix_f32(&self.0, "2.6.0") }.unwrap()
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        std::fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}

#[test]
fn mock_float32_math_and_repeated_lifecycle() {
    let mock = Mock::compile(&[]);
    let torch = mock.open();
    for _ in 0..12 {
        let mut a = torch
            .from_f32_matrix(2, 3, &[1., 2., 3., 4., 5., 6.])
            .unwrap();
        let mut b = torch
            .from_f32_matrix(3, 2, &[7., 8., 9., 10., 11., 12.])
            .unwrap();
        let mut product = a.matmul(&b).unwrap();
        assert_eq!(product.info().unwrap().shape, [2, 2]);
        assert_eq!(product.info().unwrap().dtype, F32_DTYPE_CODE);
        assert_eq!(product.to_vec_bounded(4).unwrap(), [58., 64., 139., 154.]);
        let mut guard = [-9.0; 6];
        product.copy_into(&mut guard[1..5]).unwrap();
        assert_eq!(guard, [-9., 58., 64., 139., 154., -9.]);
        assert_eq!(
            product.copy_into(&mut guard[..3]),
            Err(Error::CapacityMismatch {
                expected: 4,
                provided: 3
            })
        );
        assert_eq!(guard[0], -9.);
        product.release().unwrap();
        a.release().unwrap();
        b.release().unwrap();
        assert_eq!(product.release(), Err(Error::AlreadyReleased));
    }
}

#[test]
fn mock_float32_rejects_invalid_inputs_and_facade_faults() {
    let mock = Mock::compile(&[]);
    let torch = mock.open();
    assert_eq!(
        torch.from_f32_matrix(0, 1, &[]).err(),
        Some(Error::InvalidMatrixShape)
    );
    assert_eq!(
        torch.from_f32_matrix(usize::MAX, 2, &[]).err(),
        Some(Error::InvalidMatrixShape)
    );
    assert_eq!(
        torch.from_f32_matrix(MAX_COPY_ELEMENTS + 1, 1, &[]).err(),
        Some(Error::InvalidMatrixShape)
    );
    assert_eq!(
        torch.from_f32_matrix(1, 2, &[1.]).err(),
        Some(Error::CapacityMismatch {
            expected: 2,
            provided: 1
        })
    );
    assert_eq!(
        torch.from_f32_matrix(1, 1, &[f32::NAN]).err(),
        Some(Error::NonFinite)
    );
    assert_eq!(
        torch.from_f32_matrix(1, 1, &[f32::INFINITY]).err(),
        Some(Error::NonFinite)
    );
    let a = torch.from_f32_matrix(1, 2, &[1., 2.]).unwrap();
    let b = torch.from_f32_matrix(1, 1, &[3.]).unwrap();
    assert_eq!(
        a.matmul(&b).err(),
        Some(Error::MatrixDimensionMismatch {
            left_cols: 2,
            right_rows: 1
        })
    );
    assert_eq!(
        a.to_vec_bounded(1).err(),
        Some(Error::LengthLimitExceeded {
            reported: 2,
            maximum: 1
        })
    );
    let wrong_dtype = torch.from_f32_matrix(1, 1, &[-301.]).unwrap();
    assert_eq!(wrong_dtype.info().unwrap().dtype, 2);
    assert_eq!(
        wrong_dtype.shape(),
        Err(Error::MatrixDtypeMismatch { reported: 2 })
    );
    let wrong_shape = torch.from_f32_matrix(1, 1, &[-302.]).unwrap();
    assert_eq!(wrong_shape.info().unwrap().shape, [2, 1]);
    assert_eq!(
        wrong_shape.shape(),
        Err(Error::MatrixShapeMismatch {
            expected: [1, 1],
            reported: [2, 1]
        })
    );
    let nonfinite_copy = torch.from_f32_matrix(1, 1, &[-303.]).unwrap();
    let mut guarded = [42.];
    assert_eq!(
        nonfinite_copy.copy_into(&mut guarded),
        Err(Error::NonFinite)
    );
    assert_eq!(guarded, [42.]);
    assert!(matches!(
        torch.from_f32_matrix(1, 1, &[-304.]),
        Err(Error::Foreign { status: 7, .. })
    ));
    let alias = torch.from_f32_matrix(1, 1, &[-305.]).unwrap();
    assert!(matches!(
        alias.matmul(&b),
        Err(Error::Foreign { status: -1, .. })
    ));
}

#[test]
fn mock_float32_rejects_wrong_version_and_missing_symbol() {
    let version = Mock::compile(&["-DMOCK_F32_API_VERSION=4"]);
    assert!(matches!(
        unsafe { Torch::open_matrix_f32(&version.0, "2.6.0") },
        Err(Error::MatrixApiVersion { reported: 4 })
    ));
    let missing = Mock::compile(&["-DMOCK_MISSING_F32_COPY"]);
    assert!(matches!(
        unsafe { Torch::open_matrix_f32(&missing.0, "2.6.0") },
        Err(Error::Native(_))
    ));
}

#[test]
#[ignore = "requires CGX2_LIBTORCH_REAL_PACKAGE with a pinned CPU float32 v3 facade"]
fn real_pinned_float32_reference_and_cycles() {
    let package = std::env::var("CGX2_LIBTORCH_REAL_PACKAGE").unwrap();
    eprintln!("opening pinned float32 package");
    // SAFETY: checked_lock and closure verification precede all calls.
    let torch = unsafe { Torch::open_pinned(Path::new(&package), "torch_cpu") }.unwrap();
    eprintln!("pinned float32 package open");
    let left = [1.25_f32, -2.0, 3.5, 4.0, 0.5, -1.25];
    let right = [2.0_f32, -3.0, 0.75, 1.5, -4.0, 2.25];
    let reference: Vec<f32> = (0..2)
        .flat_map(|r| {
            (0..2).map(move |c| {
                (0..3)
                    .map(|k| left[r * 3 + k] * right[k * 2 + c])
                    .sum::<f32>()
            })
        })
        .collect();
    for cycle in 0..2 {
        eprintln!("starting pinned float32 cycle {}", cycle + 1);
        let a = torch.from_f32_matrix(2, 3, &left).unwrap();
        eprintln!("left created");
        let b = torch.from_f32_matrix(3, 2, &right).unwrap();
        eprintln!("right created");
        let result = a.matmul(&b).unwrap();
        eprintln!("product created");
        assert_eq!(result.info().unwrap().shape, [2, 2]);
        assert_eq!(result.info().unwrap().dtype, F32_DTYPE_CODE);
        for (actual, expected) in result.to_vec_bounded(4).unwrap().iter().zip(&reference) {
            assert!((actual - expected).abs() <= 1.0e-5);
        }
        eprintln!("pinned float32 cycle {} passed", cycle + 1);
    }
}
