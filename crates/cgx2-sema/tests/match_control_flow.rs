use cgx2_sema::{check_module, Type, TypeDiagnostic, TypedModule};
use cgx2_syntax::parse_module;

fn check(source: &str) -> Result<TypedModule, Vec<TypeDiagnostic>> {
    check_module(&parse_module(source).unwrap())
}

const PREFIX: &str = "variant V:\n    A\n    B\n";

#[test]
fn match_values_and_explicit_returns_have_separate_contexts() {
    for source in [
        include_str!("match_flow/nontail.cg"),
        include_str!("match_flow/nontail_value.cg"),
        include_str!("match_flow/loop.cg"),
        include_str!("match_flow/partial.cg"),
    ] {
        check(source).unwrap();
    }
    let typed = check(include_str!("match_flow/all_return.cg")).unwrap();
    assert_eq!(typed.functions["no_value"].return_type, Type::Unit);
    check(
        &include_str!("match_flow/partial.cg")
            .replace("fn choose(flag: Bool):", "fn choose(flag: Bool) -> i64:"),
    )
    .unwrap();
    assert!(
        check(&include_str!("match_flow/all_return.cg").replace("return 20", "return false"))
            .unwrap_err()
            .iter()
            .any(|d| d.code == "CGT003")
    );
    for name in ["choose", "unused"] {
        assert_eq!(typed.functions[name].return_type, Type::I64);
    }
}

#[test]
fn explicit_and_fallthrough_types_are_checked_at_their_source_spans() {
    let inferred = include_str!("match_flow/inferred.cg");
    assert!(check(inferred)
        .unwrap_err()
        .iter()
        .any(|d| d.code == "CGT003"));
    for body in [
        "    match V.A():\n        V.A:\n            return false\n        V.B:\n            2\n",
        "    match V.A():\n        V.A:\n            false\n        V.B:\n            true\n",
        "    match V.A():\n        V.A:\n            return false\n        V.B:\n            return 2\n    42\n",
        "    if true:\n        match V.A():\n            _:\n                return false\n    else:\n        0\n    42\n",
    ] {
        let source = format!("{PREFIX}# café λ\nfn wrong() -> i64:\n{body}");
        let errors = check(&source).unwrap_err();
        let mismatch = errors.iter().find(|d| d.code == "CGT003").unwrap();
        let span = source.get(mismatch.span.start..mismatch.span.end).unwrap();
        assert!(span.contains("false") || span.contains("true"), "{errors:?}");
    }
}

#[test]
fn inferred_nested_returns_and_no_value_fallthrough_remain_sound() {
    for body in [
        "    match V.A():\n        V.A:\n            if true:\n                return false\n            else:\n                0\n            1\n        V.B:\n            2\n",
        "    match V.A():\n        V.A:\n            match V.B():\n                _:\n                    return false\n        V.B:\n            2\n",
        "    match V.A():\n        V.A:\n            return\n        V.B:\n            2\n",
        "    match V.A():\n        V.A:\n            if true:\n                return 1\n            else:\n                0\n        V.B:\n            return 2\n",
        "    match V.A():\n        V.A:\n            let x = 1\n        V.B:\n            2\n",
    ] {
        let source = format!("{PREFIX}fn wrong():\n{body}");
        assert!(check(&source).unwrap_err().iter().any(|d| d.code == "CGT003"), "{source}");
    }
    let source = format!("{PREFIX}fn unit():\n    match V.A():\n        _:\n            return\n");
    assert_eq!(
        check(&source).unwrap().functions["unit"].return_type,
        Type::Unit
    );
    // Explicit returns remain checked even after a terminating match, just
    // as annotated functions and native target type checking require.
    let source = format!("{PREFIX}fn value():\n    match V.A():\n        _:\n            return 42\n    return false\n");
    assert!(check(&source)
        .unwrap_err()
        .iter()
        .any(|d| d.code == "CGT003"));
}

#[test]
fn loop_return_policy_coverage_and_unreachable_arms_are_preserved() {
    for (source, code) in [
        (format!("{PREFIX}fn wrong():\n    match V.A():\n        V.A:\n            1\n"), "CGT036"),
        (format!("{PREFIX}fn wrong():\n    match V.A():\n        _:\n            return 1\n        V.B:\n            return 2\n"), "CGT037"),
        (format!("{PREFIX}fn wrong():\n    for i in range(0, 1):\n        match V.A():\n            _:\n                return 1\n"), "CGT003"),
    ] {
        assert!(check(&source).unwrap_err().iter().any(|d| d.code == code));
    }
}

#[test]
fn list_loop_match_returns_are_inferred_but_loop_fallthrough_is_required() {
    let body = "    for x in xs:\n        match V.A():\n            _:\n                return 42\n    42\n";
    let source = format!("{PREFIX}fn choose(xs: List<i64>):\n{body}");
    assert_eq!(
        check(&source).unwrap().functions["choose"].return_type,
        Type::I64
    );
    for invalid in [
        source.replace("return 42", "return false"),
        source.replace("    42\n", ""),
    ] {
        assert!(check(&invalid)
            .unwrap_err()
            .iter()
            .any(|d| d.code == "CGT003"));
    }
}

#[test]
fn empty_arm_has_unit_value_in_existing_ast_contract() {
    // Source suites require a statement. Exercise the checker independently
    // with an empty suite to ensure absence of a value cannot bypass a join.
    let source = format!("{PREFIX}fn value() -> i64:\n    match V.A():\n        V.A:\n            1\n        V.B:\n            2\n");
    let mut module = parse_module(&source).unwrap();
    let cgx2_syntax::StmtKind::Match { arms, .. } = &mut module.functions[0].body[0].kind else {
        panic!("expected match");
    };
    arms[0].body.clear();
    let errors = check_module(&module).unwrap_err();
    assert!(errors.iter().any(|d| d.code == "CGT003"));
}
