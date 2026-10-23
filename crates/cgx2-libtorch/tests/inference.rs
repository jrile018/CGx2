use cgx2_libtorch::{Error, Torch};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Mock(PathBuf);
impl Mock {
    fn new(flags: &[&str]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "cgx2-inference-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("libmock.so");
        assert!(
            Command::new("cc")
                .args([
                    "-std=c11", "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror"
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
    fn open(&self) -> Result<Torch, Error> {
        // SAFETY: reviewed mock implements facade.h v1-v4; flags inject test failures.
        unsafe { Torch::open_inference(&self.0, "2.6.0") }
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        std::fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}

fn pipeline(torch: &Torch) -> Vec<f32> {
    let input = vec![1., -2., 3., 4., 0.5, -1.];
    let x = torch.from_f32_matrix(2, 3, &input).unwrap();
    drop(input); // creation must retain its own storage
    let w1 = torch
        .from_f32_matrix(3, 2, &[0.5, -1., 2., 0.25, -0.5, 1.5])
        .unwrap();
    let b1 = torch
        .from_f32_matrix(2, 2, &[0.25, -0.5, 0.25, -0.5])
        .unwrap();
    let hidden = x.matmul(&w1).unwrap().add(&b1).unwrap().relu().unwrap();
    assert_eq!(hidden.shape().unwrap(), [2, 2]);
    assert_eq!(hidden.info().unwrap().dtype, 1);
    assert_eq!(hidden.device().unwrap(), "cpu");
    let w2 = torch.from_f32_matrix(2, 1, &[2., -3.]).unwrap();
    let b2 = torch.from_f32_matrix(2, 1, &[1., 1.]).unwrap();
    let mut result = hidden.matmul(&w2).unwrap().add(&b2).unwrap();
    let output = result.to_vec_bounded(2).unwrap();
    result.release().unwrap();
    assert_eq!(result.release(), Err(Error::AlreadyReleased));
    output
}

fn reference() -> Vec<f32> {
    let x = [1., -2., 3., 4., 0.5, -1.];
    let w1 = [0.5, -1., 2., 0.25, -0.5, 1.5];
    (0..2)
        .map(|row| {
            let hidden: Vec<f32> = (0..2)
                .map(|col| {
                    let sum: f32 = (0..3).map(|k| x[row * 3 + k] * w1[k * 2 + col]).sum();
                    (sum + [0.25, -0.5][col]).max(0.)
                })
                .collect();
            hidden[0] * 2. - hidden[1] * 3. + 1.
        })
        .collect()
}

#[test]
fn mock_two_layer_inference_and_lifetimes() {
    let mock = Mock::new(&[]);
    let torch = mock.open().unwrap();
    for _ in 0..3 {
        assert_eq!(pipeline(&torch), reference());
    }
    let left = torch.from_f32_matrix(1, 2, &[1., -2.]).unwrap();
    let right = torch.from_f32_matrix(2, 1, &[1., 2.]).unwrap();
    assert!(matches!(
        left.add(&right),
        Err(Error::MatrixShapeMismatch { .. })
    ));
    let other = mock.open().unwrap();
    assert!(matches!(
        left.add(&other.from_f32_matrix(1, 2, &[1., 2.]).unwrap()),
        Err(Error::DifferentRuntime)
    ));
    let alias = torch.from_f32_matrix(1, 1, &[-305.]).unwrap();
    assert!(matches!(alias.relu(), Err(Error::Foreign { .. })));
    assert_eq!(alias.to_vec_bounded(1).unwrap(), [-305.]);
    let result = left.relu().unwrap();
    drop(left);
    drop(right);
    drop(alias);
    drop(torch);
    assert_eq!(result.to_vec_bounded(2).unwrap(), [1., 0.]);
}

#[test]
fn rejects_inference_version_and_missing_export() {
    let wrong = Mock::new(&["-DMOCK_INFERENCE_API_VERSION=9"]);
    assert!(matches!(
        wrong.open(),
        Err(Error::MatrixApiVersion { reported: 9 })
    ));
    let missing = Mock::new(&["-DMOCK_MISSING_RELU"]);
    assert!(matches!(missing.open(), Err(Error::Native(_))));
}

#[test]
#[ignore = "requires a prepared real v4 CPU package; set CGX2_LIBTORCH_REAL_PACKAGE"]
fn real_pinned_two_layer_inference_no_python() {
    let package = std::env::var("CGX2_LIBTORCH_REAL_PACKAGE").unwrap();
    // SAFETY: operator supplies reviewed facade.h v4 and freshly pinned ELF closure.
    let torch = unsafe { Torch::open_pinned(Path::new(&package), "torch_cpu") }.unwrap();
    for _ in 0..4 {
        let output = pipeline(&torch);
        for (actual, expected) in output.iter().zip(reference()) {
            assert!((actual - expected).abs() <= 1e-5);
        }
        println!("real_cpu_output={output:?}");
    }
    let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
    assert!(!maps.to_ascii_lowercase().contains("libpython"));
    assert!(!maps.contains("/python3"));
    println!("no_libpython=true; device=cpu; dtype=f32; tolerance=1e-5");
}
