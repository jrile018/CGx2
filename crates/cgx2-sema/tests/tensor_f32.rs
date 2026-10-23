use cgx2_sema::{check_module, tensor_profile, TensorOperation};
use cgx2_syntax::parse_module;

const SOURCE: &str = "unsafe foreign native pub fn matmul_f32_cpu(left: String, right: String) -> String effects(Foreign) caps(native) = torch_cpu.cgx2_torch_matmul_f32 runtime \"cgx2.libtorch.cpu@2.6.0\" ownership copied\n\nfn main() -> String effects(Foreign) caps(native):\n    matmul_f32_cpu(\"[[0.1, 1e-40]]\", \"[[1.0], [2.0]]\")\n";

#[test]
fn checked_float32_literals_round_and_preserve_subnormals() {
    let module = parse_module(SOURCE).unwrap();
    check_module(&module).unwrap();
    let profile = tensor_profile(&module).unwrap().unwrap();
    let TensorOperation::MatrixMultiplyF32 { left, right } = profile.operation else {
        panic!("expected float32 matrix operation");
    };
    assert_eq!((left.rows, left.cols), (1, 2));
    assert_eq!((right.rows, right.cols), (2, 1));
    assert_eq!(left.values[0], 0.1_f32);
    assert_ne!(f64::from(left.values[0]), 0.1_f64);
    assert!(left.values[1].is_subnormal());
}

#[test]
fn float32_overflow_underflow_and_shape_errors_are_located() {
    for (bad, message) in [
        ("1e40", "overflows"),
        ("1e-50", "underflows"),
        ("NaN", "finite"),
    ] {
        let source = SOURCE.replace("0.1", bad);
        let errors = tensor_profile(&parse_module(&source).unwrap()).unwrap_err();
        assert_eq!(errors[0].code, "CGT018");
        assert!(errors[0].message.contains(message), "{errors:?}");
        assert!(errors[0].span.start < errors[0].span.end);
    }
    let source = SOURCE.replace("[[1.0], [2.0]]", "[[1.0]]");
    let errors = tensor_profile(&parse_module(&source).unwrap()).unwrap_err();
    assert_eq!(errors[0].code, "CGT018");
    assert!(errors[0].message.contains("left columns"));
}
