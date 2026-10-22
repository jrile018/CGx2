use cgx2_emit_python::emit_python;
use cgx2_sema::check_module;
use cgx2_syntax::{parse_module, Span};
use std::process::Command;

fn checked(source: &str) -> (cgx2_syntax::Module, cgx2_sema::TypedModule) {
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    (module, typed)
}

fn run(source: &str, args: &[&str]) -> std::process::Output {
    let (module, typed) = checked(source);
    let python = emit_python(&module, &typed).unwrap();
    let path = std::env::temp_dir().join(format!(
        "cgx2-emitter-{}-{}.py",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, python).unwrap();
    let compile = Command::new("python3")
        .arg("-m")
        .arg("py_compile")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new("python3")
        .arg(&path)
        .args(args)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    output
}

#[test]
fn emits_script_with_mutation_and_checked_arithmetic() {
    let output = run("let mut x = 2\nx += 3\nx * 4\n", &[]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"20\n");
}

#[test]
fn emits_function_calls_and_tail_returns() {
    let output = run(
        "fn twice(x: i64) -> i64:\n    x * 2\n\nfn main() -> i64:\n    twice(7)\n",
        &[],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"14\n");
}

#[test]
fn rejects_unsupported_constructs_at_their_spans() {
    let cases = [
        ("for x in 1:\n    x\n", "for x in 1:\n    x\n"),
        ("fn main(xs: List<i64>):\n    1\n", "List<i64>"),
        ("app.value\n", "app.value"),
        ("app.value()\n", "app.value"),
    ];
    for (source, needle) in cases {
        let module = parse_module(source).unwrap();
        let typed = check_module(&module)
            .unwrap_or_else(|_| check_module(&parse_module("1\n").unwrap()).unwrap());
        let error = emit_python(&module, &typed).unwrap_err();
        let start = source.find(needle).unwrap();
        assert_eq!(error[0].code, "CGE001");
        assert_eq!(
            error[0].span,
            Span {
                start,
                end: start + needle.len()
            }
        );
    }
}

#[test]
fn checked_i64_boundaries_and_division() {
    for source in [
        "9223372036854775807 + 1\n",
        "let min = 0 - 9223372036854775807 - 1\nmin / (0 - 1)\n",
    ] {
        let output = run(source, &[]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("i64 arithmetic overflow"));
    }
    let output = run("1 / 0\n", &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("division by zero"));
    let output = run("(0 - 7) / 3\n", &[]);
    assert_eq!(output.stdout, b"-2\n");
}

#[test]
fn arguments_and_boolean_display_keep_cgx2_values() {
    assert_eq!(run("arg_count()\n", &["a", "b"]).stdout, b"2\n");
    assert_eq!(run("arg(0)\n", &["hello world"]).stdout, b"hello world\n");
    assert_eq!(run("true\n", &[]).stdout, b"true\n");
    assert!(!run("arg(0)\n", &[]).status.success());
}

#[test]
fn escaped_unicode_and_keyword_helper_names_survive() {
    assert_eq!(
        run("\"line\\nquote\\\"slash\\\\café\"\n", &[]).stdout,
        "line\nquote\"slash\\café\n".as_bytes()
    );
    let source =
        "fn class(_cgx2_add: i64) -> i64:\n    _cgx2_add + 1\n\nfn main() -> i64:\n    class(4)\n";
    assert_eq!(run(source, &[]).stdout, b"5\n");
}

#[test]
fn stale_summary_and_out_of_range_literals_are_rejected() {
    let (module, _) = checked("true\n");
    let (_, stale) = checked("1234\n");
    let error = emit_python(&module, &stale).unwrap_err();
    assert_eq!(error[0].code, "CGE002");
    assert_eq!(error[0].span, module.span);

    let out_of_range = parse_module("9223372036854775808\n").unwrap();
    assert_eq!(check_module(&out_of_range).unwrap_err()[0].code, "CGT007");
}

#[test]
fn manually_mixed_items_are_not_silently_dropped() {
    let (mut module, _) = checked("fn main() -> i64:\n    1\n");
    let script = parse_module("2\n").unwrap();
    module.statements = script.statements;
    let typed = check_module(&module).unwrap();
    let error = emit_python(&module, &typed).unwrap_err();
    assert_eq!(error[0].code, "CGE001");
}

#[test]
fn invalid_string_escape_has_literal_diagnostic() {
    let source = "\"bad\\q\"\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let error = emit_python(&module, &typed).unwrap_err();
    assert_eq!(error[0].code, "CGE003");
    assert_eq!(error[0].span, Span { start: 0, end: 7 });
}
