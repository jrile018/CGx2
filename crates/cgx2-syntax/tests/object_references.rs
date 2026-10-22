use cgx2_syntax::{format_module, parse_module};

#[test]
fn object_intrinsics_round_trip_through_formatter() {
    let source = include_str!("../../cgx2-cli/tests/fixtures/object_references.cg");
    let module = parse_module(source).unwrap();
    assert_eq!(format_module(&module), source);
    assert_eq!(module.statements.len(), 13);
}

#[test]
fn nested_no_gc_object_intrinsics_format_stably() {
    let source = include_str!("../../cgx2-cli/tests/fixtures/object_nested_no_gc.cg");
    let module = parse_module(source).unwrap();
    assert_eq!(format_module(&module), source);
}
