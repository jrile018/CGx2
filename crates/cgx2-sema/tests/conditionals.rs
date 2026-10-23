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
