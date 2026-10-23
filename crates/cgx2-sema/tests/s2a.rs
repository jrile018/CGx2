use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

fn errors(source: &str) -> Vec<cgx2_sema::TypeDiagnostic> {
    let module = parse_module(source).unwrap();
    let errors = check_module(&module).unwrap_err();
    for error in &errors {
        assert!(
            source.get(error.span.start..error.span.end).is_some(),
            "{error:?}"
        );
    }
    errors
}
const BASE: &str = "variant Reply:\n    Empty\n    Value(i64)\nfn main() -> i64:\n    match Reply.Value(42):\n        Reply.Empty:\n            0\n        Reply.Value(x):\n            x\n";

#[test]
fn rejects_invalid_coverage_payloads_scopes_and_arm_types() {
    for (source, code) in [
        (
            BASE.replace("        Reply.Empty:\n            0\n", ""),
            "CGT036",
        ),
        (
            BASE.replace(
                "        Reply.Empty:\n            0\n",
                "        Reply.Value(y):\n            y\n",
            ),
            "CGT037",
        ),
        (BASE.replace("Reply.Value(42)", "Reply.Value()"), "CGT034"),
        (
            BASE.replace("Reply.Value(42)", "Reply.Value(true)"),
            "CGT003",
        ),
        (
            BASE.replace("        Reply.Empty:", "        Reply.Unknown:"),
            "CGT033",
        ),
        (
            BASE.replace("            0\n", "            true\n"),
            "CGT003",
        ),
        (
            BASE.replace("            x\n", "            absent\n"),
            "CGT001",
        ),
        (
            BASE.replace("    Value(i64)", "    Value(i64, Bool)")
                .replace("Reply.Value(42)", "Reply.Value(42, true)")
                .replace("Reply.Value(x):", "Reply.Value(x, x):"),
            "CGT001",
        ),
        (BASE.replace("        Reply.Empty:", "        _:"), "CGT037"),
    ] {
        assert!(
            errors(&source).iter().any(|e| e.code == code),
            "{code}: {source}"
        );
    }
}

#[test]
fn unrelated_nominal_types_never_match_even_with_same_cases() {
    let source = format!(
        "variant Other:\n    Empty\n    Value(i64)\n{}",
        BASE.replace("        Reply.", "        Other.")
    );
    assert!(errors(&source).iter().any(|e| e.code == "CGT003"));
}

#[test]
fn recursive_shapes_and_generic_arity_fail_closed() {
    for source in [
        "variant Node:\n    Next(Node)\n",
        "record Node:\n    next: Option<Node>\n",
        "variant Left:\n    Next(Right)\nrecord Right:\n    previous: Left\n",
    ] {
        assert!(errors(source).iter().any(|e| e.code == "CGT030"));
    }
    for ty in [
        "Option",
        "Option<i64, Bool>",
        "Result<i64>",
        "Result",
        "Option<Unknown>",
    ] {
        let source = format!("fn wrong(x: {ty}) -> i64:\n    0\n");
        assert!(!errors(&source).is_empty());
    }
}

#[test]
fn private_types_cannot_leak_through_public_aggregates_or_applications() {
    for tail in [
        "pub variant Exposed:\n    Leak(Hidden)\n",
        "pub record Exposed:\n    leak: Option<Hidden>\n",
        "pub fn leak(x: Result<i64, Hidden>) -> i64:\n    0\n",
    ] {
        assert!(errors(&format!("variant Hidden:\n    Case\n{tail}"))
            .iter()
            .any(|e| e.code == "CGT022"));
    }
}

#[test]
fn deferred_syntax_has_explicit_semantic_diagnostics() {
    for source in [
        "pub variant Box<T: Eq + Show>:\n    Value(T)\n",
        "fn identity<T: Eq>(x: T) -> T:\n    x\n",
        "record Box<T>:\n    value: T\n",
        "fn main() -> i64:\n    let values = [1, 2]\n    values[0]\n",
        "fn main() -> i64:\n    for i in range(0, 1):\n        invariant i >= 0\n        decreases 1 - i\n        i\n    0\n",
    ] { assert!(errors(source).iter().any(|e| e.code == "CGT032")); }
}

#[test]
fn new_diagnostics_preserve_utf8_payload_and_binding_spans() {
    let source="variant Café:\n    Valeur(i64)\nfn main() -> i64:\n    match Café.Valeur(true):\n        Café.Valeur(été):\n            absent\n";
    let errors = errors(source);
    let payload = errors.iter().find(|e| e.code == "CGT003").unwrap();
    assert_eq!(&source[payload.span.start..payload.span.end], "true");
    let scope = errors.iter().find(|e| e.code == "CGT001").unwrap();
    assert_eq!(&source[scope.span.start..scope.span.end], "absent");
}
