use cgx2_sema::{check_module, Type};
use cgx2_syntax::parse_module;

fn check(source: &str) -> Result<cgx2_sema::TypedModule, Vec<cgx2_sema::TypeDiagnostic>> {
    check_module(&parse_module(source).unwrap())
}

#[test]
fn explicit_returns_infer_and_join_but_branch_tails_are_unit() {
    let source = "fn choose(flag: Bool):\n    if flag:\n        return 1\n    else:\n        return 2\nfn main() -> i64:\n    choose(true)\n";
    assert_eq!(
        check(source).unwrap().functions["choose"].return_type,
        Type::I64
    );
    check(
        "fn f(flag: Bool) -> i64:\n    if flag:\n        return 1\n    else:\n        2\n    3\n",
    )
    .unwrap();
    check("fn f(flag: Bool) -> Unit:\n    if flag:\n        1\n    else:\n        2\n").unwrap();
    check(include_str!("../../cgx2-cli/examples/conditionals.cg")).unwrap();
}

#[test]
fn both_branches_are_checked_and_scopes_do_not_escape() {
    for (source, code) in [
        ("fn f() -> i64:\n    if true:\n        return 1\n    else:\n        2\n", "CGT003"),
        ("fn f() -> i64:\n    if 1:\n        return 1\n    else:\n        return 2\n", "CGT003"),
        ("fn f() -> i64:\n    if true:\n        return 1\n    else:\n        return false\n", "CGT003"),
        ("fn f() -> i64:\n    if true:\n        let x = 1\n    else:\n        x\n    x\n", "CGT001"),
        ("fn f() -> i64:\n    let x = 1\n    if false:\n        x = 2\n    else:\n        0\n    x\n", "CGT004"),
        ("fn f() effects() caps():\n    if false:\n        arg_count()\n    else:\n        0\n", "CGT012"),
    ] {
        let errors = check(source).unwrap_err();
        assert!(errors.iter().any(|e| e.code == code), "{errors:?}");
    }
}

#[test]
fn conditional_memory_is_rejected_even_nested_or_inactive() {
    for source in [
        "if false:\n    let r = memory.region(mode: manual, limit: 16)\nelse:\n    0\n",
        "let r = memory.region(mode: manual, limit: 16)\nno_gc(budget: 8):\n    if true:\n        1\n    else:\n        2\n",
        "fn f() -> Bool:\n    if false:\n        memory.collect(r)\n    else:\n        return true\n",
    ] {
        let error = check(source).unwrap_err().remove(0);
        assert_eq!(error.code, "CGM001");
        assert!(error.message.contains("conditional memory"));
        assert!(source[error.span.start..error.span.end].starts_with("if"));
    }
}
