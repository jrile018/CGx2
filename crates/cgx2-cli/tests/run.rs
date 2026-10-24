use cgx2_cli::{run_source, Value};

fn value(source: &str) -> Value {
    run_source(source, &[]).unwrap().value
}

#[test]
fn bounded_source_contracts_gate_body_and_return() {
    let true_contract =
        "fn main() -> i64 effects() caps():\n    requires true\n    ensures result == 7\n    7\n";
    assert_eq!(value(true_contract), Value::I64(7));
    let pre = "fn main() -> i64:\n    requires false\n    9223372036854775807 + 1\n";
    let error = run_source(pre, &[]).unwrap_err();
    assert_eq!(error.code, "CGR015");
    assert_eq!(
        &pre[error.span.unwrap().start..error.span.unwrap().end],
        "false"
    );
    let post = "fn main() -> i64:\n    ensures result == 8\n    7\n";
    let error = run_source(post, &[]).unwrap_err();
    assert_eq!(error.code, "CGR016");
    assert_eq!(
        &post[error.span.unwrap().start..error.span.unwrap().end],
        "result == 8"
    );
}

#[test]
fn pure_contract_calls_execute_with_existing_step_limit() {
    let source = "fn positive(x: i64) -> Bool effects() caps():\n    x > 0\nfn main() -> i64 effects() caps():\n    requires positive(2)\n    7\n";
    assert_eq!(value(source), Value::I64(7));
    let mut source = String::new();
    source.push_str("fn f0() -> Bool:\n    true\n");
    for i in 1..22 {
        source.push_str(&format!(
            "fn f{i}() -> Bool:\n    f{}() == f{}()\n",
            i - 1,
            i - 1
        ));
    }
    source.push_str("fn main() -> i64:\n    requires f21()\n    1\n");
    let error = run_source(&source, &[]).unwrap_err();
    assert_eq!(error.code, "CGR010");
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
fn low_level_run_source_checks_memory_before_interpreting() {
    let source = "let r = memory.region(mode: manual, limit: 128)\nlet x = memory.alloc_bytes(r, 1)\nmemory.drop(x)\nmemory.collect(r)\nmemory.close(r)\n";
    assert_eq!(run_source(source, &[]).unwrap().value, Value::Unit);
    let error = run_source(
        "let x = memory.owned_bytes(1)\nlet y = memory.move(x)\nx\n",
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code, "CGO001");
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
        "CGR004"
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
