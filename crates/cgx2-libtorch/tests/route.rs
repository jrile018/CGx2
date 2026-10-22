use cgx2_libtorch::{Error, Torch};
use std::path::Path;
use std::process::Command;

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
    let status = Command::new("cc")
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fsyntax-only",
            "-x",
            "c",
        ])
        .arg(root.join("facade/facade.h"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
#[ignore = "requires a real version-pinned LibTorch facade; see README"]
fn real_libtorch_cpu_when_installed() {
    let path = std::env::var("CGX2_LIBTORCH_FACADE").expect("set CGX2_LIBTORCH_FACADE");
    let version = std::env::var("CGX2_LIBTORCH_VERSION").expect("set CGX2_LIBTORCH_VERSION");
    let dependencies = Command::new("ldd").arg(&path).output().expect("ldd");
    assert!(dependencies.status.success());
    assert!(!String::from_utf8_lossy(&dependencies.stdout).contains("libpython"));
    let torch = Torch::open(Path::new(&path), &version).unwrap();
    let mut x = torch.from_f64(&[1.0, 2.0, 3.0]).unwrap();
    let y = torch.from_f64(&[4.0, 5.0, 6.0]).unwrap();
    let sum = x.add(&y).unwrap();
    assert_eq!(sum.shape().unwrap(), [3]);
    assert_eq!(sum.dtype(), "float64");
    assert_eq!(sum.device(), "cpu");
    assert_eq!(sum.to_vec().unwrap(), [5.0, 7.0, 9.0]);
    x.release().unwrap();
    assert_eq!(x.release(), Err(Error::AlreadyReleased));
    assert_eq!(
        Torch::open(Path::new(&path), "deliberately-wrong-version").err(),
        Some(Error::Native(cgx2_ffi_native::Error::VersionMismatch))
    );
}
