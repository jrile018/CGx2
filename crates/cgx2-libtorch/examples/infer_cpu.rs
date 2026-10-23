use cgx2_libtorch::Torch;
use std::path::Path;
fn main() {
    let package = std::env::args()
        .nth(1)
        .expect("usage: infer_cpu PINNED_V4_PACKAGE");
    // SAFETY: operator has reviewed facade.h and prepared its exact dependency closure.
    let torch = unsafe { Torch::open_pinned(Path::new(&package), "torch_cpu") }
        .expect("pinned inference facade");
    let x = torch
        .from_f32_matrix(2, 3, &[1., -2., 3., 4., 0.5, -1.])
        .unwrap();
    let w1 = torch
        .from_f32_matrix(3, 2, &[0.5, -1., 2., 0.25, -0.5, 1.5])
        .unwrap();
    let b1 = torch
        .from_f32_matrix(2, 2, &[0.25, -0.5, 0.25, -0.5])
        .unwrap();
    let hidden = x.matmul(&w1).unwrap().add(&b1).unwrap().relu().unwrap();
    let w2 = torch.from_f32_matrix(2, 1, &[2., -3.]).unwrap();
    let b2 = torch.from_f32_matrix(2, 1, &[1., 1.]).unwrap();
    let output = hidden.matmul(&w2).unwrap().add(&b2).unwrap();
    println!(
        "device={} dtype={} shape={:?} output={:?}",
        output.device().unwrap(),
        output.info().unwrap().dtype,
        output.shape().unwrap(),
        output.to_vec_bounded(2).unwrap()
    );
    assert!(
        !std::fs::read_to_string("/proc/self/maps")
            .unwrap()
            .to_ascii_lowercase()
            .contains("libpython")
    );
}
