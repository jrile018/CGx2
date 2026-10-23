use cgx2_cli::{run_checked_source, Value};

#[test]
fn selected_branch_nested_scopes_outer_mutation_and_unit_fallthrough() {
    let source = include_str!("../examples/conditionals.cg");
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value,
        Value::I64(42)
    );
    let source = "fn choose(flag: Bool) -> i64:\n    if flag:\n        return 3\n    else:\n        return 7\nfn main() -> i64:\n    choose(false)\n";
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value,
        Value::I64(7)
    );
    let source = "fn f(flag: Bool) -> i64:\n    if flag:\n        return 1\n    else:\n        2\n    42\nfn main() -> i64:\n    f(false)\n";
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value,
        Value::I64(42)
    );
}

#[test]
fn taken_failure_has_exact_utf8_span_and_source_order() {
    let source = "fn main() -> i64:\n    let 名 = true\n    if 名:\n        return 1 / 0\n    else:\n        return 9223372036854775807 + 1\n";
    for (source, code, expression) in [
        (source.to_owned(), "CGR006", "1 / 0"),
        (
            source.replace("let 名 = true", "let 名 = false"),
            "CGR005",
            "9223372036854775807 + 1",
        ),
    ] {
        let error = run_checked_source(&source, &[]).unwrap_err().remove(0);
        assert_eq!(error.code, code);
        let span = error.span.unwrap();
        assert_eq!(&source[span.start..span.end], expression);
    }
}

#[test]
fn nested_record_returns_and_untaken_failure() {
    let source = "record R:\n    flag: Bool\n    n: i64\nfn pick(r: R) -> R:\n    if r.flag:\n        if true:\n            return r\n        else:\n            return R(flag: false, n: 1 / 0)\n    else:\n        return R(flag: true, n: 7)\nfn main() -> i64:\n    let r = pick(R(flag: true, n: 42))\n    r.n\n";
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value,
        Value::I64(42)
    );
}
