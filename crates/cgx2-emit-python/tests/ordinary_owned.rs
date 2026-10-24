#[test]
fn ownership_rejects_before_even_a_forged_finite_value_flag() {
    let m = cgx2_syntax::parse_module(include_str!(
        "../../cgx2-syntax/ordinary-owned-evidence/fixtures/functions.cg"
    ))
    .unwrap();
    let copied = cgx2_syntax::parse_module("fn main() -> i64:\n    1\n").unwrap();
    let mut typed = cgx2_sema::check_module(&copied).unwrap();
    typed.finite_values = true;
    assert_eq!(
        cgx2_emit_python::emit_python(&m, &typed).unwrap_err()[0].code,
        "CGE006"
    );
}
