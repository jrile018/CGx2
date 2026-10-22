use cgx2_cli::{run_source, Value};

fn value(source: &str) -> Value {
    run_source(source, &[]).unwrap().value
}

#[test]
fn literal_tail_values_and_precedence() {
    assert_eq!(value("1 + 2 * 3\n"), Value::I64(7));
    assert_eq!(value("(1 + 2) * 3\n"), Value::I64(9));
    assert_eq!(value("true\n"), Value::Bool(true));
    assert_eq!(value("\"hello\"\n"), Value::String("hello".into()));
    assert_eq!(value("let x = 1\n"), Value::Unit);
}

#[test]
fn let_and_mutable_add_assign() {
    assert_eq!(value("let mut x = 2\nx += 3\nx\n"), Value::I64(5));
}

#[test]
fn function_call_return_and_local_scope() {
    let source = "fn add(x: i64, y: i64) -> i64:\n    return x + y\n\nfn main() -> i64:\n    let x = 4\n    add(x, 5)\n";
    assert_eq!(value(source), Value::I64(9));
}

#[test]
fn script_and_function_module_entry() {
    assert_eq!(value("let x = 2\nx + 3\n"), Value::I64(5));
    assert_eq!(
        value("module demo\n\nfn main() -> i64:\n    8\n"),
        Value::I64(8)
    );
}

#[test]
fn errors_have_stable_codes_and_spans() {
    for (source, code) in [
        ("9223372036854775807 + 1\n", "CGR005"),
        ("1 / 0\n", "CGR006"),
        ("true + 1\n", "CGR004"),
        ("let x = 1\nx = 2\n", "CGR003"),
        ("missing\n", "CGR001"),
        ("missing()\n", "CGR002"),
        ("fn helper():\n    1\n", "CGR008"),
        ("arg(0)\n", "CGR007"),
        ("fn main():\n    main()\n", "CGR011"),
        ("for x in 1:\n    x\n", "CGR009"),
    ] {
        let error = run_source(source, &[]).unwrap_err();
        assert_eq!(error.code, code, "{source}: {error:?}");
        assert!(error.span.is_some(), "{source}: {error:?}");
    }
}

#[test]
fn argument_builtins_and_function_arity() {
    assert_eq!(
        run_source("arg(0)\n", &["hello world".into()])
            .unwrap()
            .value,
        Value::String("hello world".into())
    );
    assert_eq!(
        run_source("arg_count()\n", &["a".into(), "b".into()])
            .unwrap()
            .value,
        Value::I64(2)
    );
    assert_eq!(run_source("arg(0 - 1)\n", &[]).unwrap_err().code, "CGR007");
    assert_eq!(run_source("arg(true)\n", &[]).unwrap_err().code, "CGR004");
    assert_eq!(
        run_source("arg_count(1)\n", &[]).unwrap_err().code,
        "CGR012"
    );
}

#[test]
fn syntax_errors_keep_cgs_code() {
    assert_eq!(run_source("let x =\n", &[]).unwrap_err().code, "CGS103");
}

#[test]
fn low_level_run_source_rejects_memory_before_interpreting() {
    let source = "let r = memory.region(mode: manual, limit: 4)\n";
    let error = run_source(source, &[]).unwrap_err();
    assert_eq!(error.code, "CGR009");
    assert!(error.message.contains("memory execution"));
}

#[test]
fn annotations_and_unsupported_items_are_explicit() {
    assert_eq!(
        run_source("let x: i64 = true\n", &[]).unwrap_err().code,
        "CGR004"
    );
    assert_eq!(
        run_source("let mut x: i64 = 1\nx = true\n", &[])
            .unwrap_err()
            .code,
        "CGR004"
    );
    assert_eq!(
        run_source("let mut x = 1\nx = true\n", &[])
            .unwrap_err()
            .code,
        "CGR004"
    );
    assert_eq!(
        run_source("let x: List<i64> = 1\n", &[]).unwrap_err().code,
        "CGR009"
    );
    assert_eq!(
        run_source(
            "fn unused():\n    for x in 1:\n        x\nfn main():\n    1\n",
            &[]
        )
        .unwrap_err()
        .code,
        "CGR009"
    );
    assert_eq!(
        run_source("fn arg(x: i64):\n    x\nfn main():\n    1\n", &[])
            .unwrap_err()
            .code,
        "CGR009"
    );
}

#[test]
fn checked_division_overflow_and_function_return_type() {
    let min_over_negative_one = "let min = 0 - 9223372036854775807 - 1\nmin / (0 - 1)\n";
    assert_eq!(
        run_source(min_over_negative_one, &[]).unwrap_err().code,
        "CGR005"
    );
    assert_eq!(
        run_source("fn main() -> i64:\n    true\n", &[])
            .unwrap_err()
            .code,
        "CGR004"
    );
    assert_eq!(
        run_source("9223372036854775808\n", &[]).unwrap_err().code,
        "CGR005"
    );
}
