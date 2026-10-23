use cgx2_syntax::{format_module, parse_module};

const SOURCE: &str = include_str!("../../cgx2-cli/tests/fixtures/tensor-cpu/src/main.cg");
const MATRIX_SOURCE: &str = include_str!("../../cgx2-cli/tests/fixtures/matrix-cpu/main.cg");

#[test]
fn direct_tensor_source_has_one_canonical_parse_and_format() {
    let parsed = parse_module(SOURCE).unwrap();
    assert_eq!(format_module(&parsed), SOURCE);
    assert_eq!(parse_module(&format_module(&parsed)).unwrap(), parsed);
}

#[test]
fn matrix_source_has_one_canonical_parse_and_format() {
    let parsed = parse_module(MATRIX_SOURCE).unwrap();
    assert_eq!(format_module(&parsed), MATRIX_SOURCE);
    assert_eq!(parse_module(&format_module(&parsed)).unwrap(), parsed);
}
