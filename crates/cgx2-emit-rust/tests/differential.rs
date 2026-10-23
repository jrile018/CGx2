use cgx2_cli::{Value, run_source};
use cgx2_emit_rust::emit_rust;
use cgx2_sema::check_module;
use cgx2_syntax::{Span, parse_module};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

fn compile_and_run(source: &str, args: &[&str], optimized: bool) -> std::process::Output {
    let module = parse_module(source).unwrap();
    let checked = check_module(&module).unwrap();
    let generated = emit_rust(&module, &checked).unwrap();
    assert_eq!(generated, emit_rust(&module, &checked).unwrap());
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "cgx2-emit-rust-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    let source_path = dir.join("generated.rs");
    let binary_path = dir.join("generated");
    fs::write(&source_path, generated).unwrap();
    let mut rustc = Command::new("rustc");
    rustc
        .arg("--edition=2024")
        .arg(&source_path)
        .arg("-o")
        .arg(&binary_path);
    if optimized {
        rustc.arg("-O");
    }
    let compiled = rustc.output().unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = Command::new(&binary_path).args(args).output().unwrap();
    fs::remove_dir_all(dir).unwrap();
    output
}

#[test]
fn source_python_foreign_binding_is_not_silently_emitted() {
    let source = "foreign python fn host(x: i64) -> i64 effects(Foreign) caps(ForeignCall) = math.isqrt runtime \"local:cpython\" ownership copied\n\nfn main() -> i64 effects(Foreign) caps(ForeignCall):\n    host(81)\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let errors = emit_rust(&module, &typed).unwrap_err();
    assert_eq!(errors[0].code, "CGRS001");
    assert_eq!(
        &source[errors[0].span.start..errors[0].span.end],
        "math.isqrt"
    );
}

fn compare(source: &str, args: &[&str], expected: &str, error: Option<&str>) {
    let owned_args = args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let cli = run_source(source, &owned_args);
    let cli_stdout = match &cli {
        Ok(result) if result.value != Value::Unit => format!("{}\n", result.value),
        _ => String::new(),
    };
    assert_eq!(cli_stdout, expected);
    assert_eq!(cli.is_err(), error.is_some());
    for optimized in [false, true] {
        let rust = compile_and_run(source, args, optimized);
        assert_eq!(rust.status.success(), error.is_none(), "{source}");
        assert_eq!(rust.stdout, expected.as_bytes(), "{source}");
        let stderr = String::from_utf8_lossy(&rust.stderr);
        if let Some(message) = error {
            assert!(stderr.contains(message), "{stderr}");
            assert!(cli.as_ref().unwrap_err().message.contains(message));
            assert!(!stderr.contains("panicked at"), "{stderr}");
        } else {
            assert!(stderr.is_empty(), "{stderr}");
        }
    }
}

#[test]
fn scalar_scripts_and_mutation_match_cli() {
    compare(
        "let mut total = 2\ntotal += 3\ntotal * 4\n",
        &[],
        "20\n",
        None,
    );
    compare("let greeting = \"unused\"\ntrue\n", &[], "true\n", None);
    compare("let x = 1\n", &[], "", None);
}

#[test]
fn functions_tail_returns_arguments_and_names_match_cli() {
    compare(
        "fn twice(x: i64) -> i64:\n    x * 2\n\nfn main() -> i64:\n    twice(7)\n",
        &[],
        "14\n",
        None,
    );
    compare(
        "fn match(_cgx2_add: i64) -> i64:\n    return _cgx2_add + 1\n\nfn main() -> i64:\n    match(4)\n",
        &[],
        "5\n",
        None,
    );
    compare("arg_count()\n", &["a", "b"], "2\n", None);
    compare("arg(0)\n", &["hello 世界"], "hello 世界\n", None);
    compare(
        "fn café(x: String) -> String:\n    x\n\nfn main() -> String:\n    café(\"été\")\n",
        &[],
        "été\n",
        None,
    );
    compare(
        "\"line\\nquote\\\"slash\\\\café\"\n",
        &[],
        "line\nquote\"slash\\café\n",
        None,
    );
}

#[test]
fn checked_arithmetic_and_argument_errors_match_cli() {
    compare(
        "9223372036854775807 + 1\n",
        &[],
        "",
        Some("i64 arithmetic overflow"),
    );
    compare(
        "9223372036854775807 * 2\n",
        &[],
        "",
        Some("i64 arithmetic overflow"),
    );
    compare(
        "let mut x = 9223372036854775807\nx += 1\nx\n",
        &[],
        "",
        Some("i64 arithmetic overflow"),
    );
    compare(
        "let min = 0 - 9223372036854775807 - 1\nmin / (0 - 1)\n",
        &[],
        "",
        Some("i64 arithmetic overflow"),
    );
    compare(
        "0 - 9223372036854775807 - 1\n",
        &[],
        "-9223372036854775808\n",
        None,
    );
    compare("1 / 0\n", &[], "", Some("division by zero"));
    compare("(0 - 7) / 3\n", &[], "-2\n", None);
    compare("arg(0)\n", &[], "", Some("out of range"));
    compare("arg(0 - 1)\n", &[], "", Some("nonnegative"));
}

#[test]
fn native_contracts_match_cli_and_preserve_error_order() {
    compare(
        "fn main() -> i64 effects() caps():\n    requires true\n    ensures result == 7\n    7\n",
        &[],
        "7\n",
        None,
    );
    compare(
        "fn main() -> i64:\n    requires false\n    9223372036854775807 + 1\n",
        &[],
        "",
        Some("requires condition failed"),
    );
    compare(
        "fn main() -> i64:\n    ensures result == 8\n    7\n",
        &[],
        "",
        Some("ensures condition failed"),
    );
}

#[test]
fn unsupported_pure_contract_call_has_source_span() {
    let source = "fn positive(x: i64) -> Bool effects() caps():\n    x > 0\nfn main() -> i64 effects() caps():\n    requires positive(2)\n    7\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let errors = emit_rust(&module, &typed).unwrap_err();
    assert_eq!(errors[0].code, "CGRS001");
    assert!(errors[0].message.contains("contract expression"));
    let start = source.find("positive(2)").unwrap();
    assert_eq!(
        errors[0].span,
        Span {
            start,
            end: start + "positive(2)".len()
        }
    );
}

#[test]
fn unsupported_and_stale_sources_have_spans() {
    let valid = parse_module("1\n").unwrap();
    let summary = check_module(&valid).unwrap();
    for (source, needle) in [
        ("for x in 1:\n    x\n", "for x in 1:\n    x\n"),
        ("app.value()\n", "app.value"),
        ("app.value\n", "app.value"),
        ("arg(x: 1)\n", "x: 1"),
        ("mystery()\n", "mystery"),
    ] {
        let module = parse_module(source).unwrap();
        let error = emit_rust(&module, &summary).unwrap_err();
        assert_eq!(error[0].code, "CGRS001", "{source}");
        assert_eq!(
            error[0].span,
            Span {
                start: source.find(needle).unwrap(),
                end: source.find(needle).unwrap() + needle.len()
            },
            "{source}"
        );
    }
    let source = "true\n";
    let module = parse_module(source).unwrap();
    let error = emit_rust(&module, &summary).unwrap_err();
    assert_eq!(error[0].code, "CGRS002");
    assert_eq!(error[0].span, module.span);

    let (module, mut summary) = {
        let module = parse_module("pub fn main() -> i64:\n    1\n").unwrap();
        let summary = check_module(&module).unwrap();
        (module, summary)
    };
    summary.functions.get_mut("main").unwrap().public = false;
    assert_eq!(emit_rust(&module, &summary).unwrap_err()[0].code, "CGRS002");
}

#[test]
fn malformed_literals_and_recursive_calls_are_rejected() {
    let module = parse_module("\"bad\\q\"\n").unwrap();
    let summary = check_module(&module).unwrap();
    let error = emit_rust(&module, &summary).unwrap_err();
    assert_eq!(error[0].code, "CGRS003");
    assert_eq!(error[0].span, Span { start: 0, end: 7 });

    let source = "fn recur(x: i64) -> i64:\n    recur(x)\n\nfn main() -> i64:\n    recur(1)\n";
    let module = parse_module(source).unwrap();
    let summary = check_module(&module).unwrap();
    let error = emit_rust(&module, &summary).unwrap_err();
    assert_eq!(error[0].code, "CGRS001");
    assert!(error[0].message.contains("recursive"));

    let mut mixed = parse_module("fn main() -> i64:\n    1\n").unwrap();
    mixed.statements = parse_module("2\n").unwrap().statements;
    let summary = check_module(&mixed).unwrap();
    let error = emit_rust(&mixed, &summary).unwrap_err();
    assert_eq!(error[0].code, "CGRS001");
    assert_eq!(error[0].span, mixed.statements[0].span);
}

#[test]
fn conditionals_preserve_branch_scope_unit_and_first_failure() {
    compare("if false:\n    1 / 0\nelse:\n    42\n", &[], "", None);
    compare(
        "let mut result = 0\nif true:\n    result = 42\nelse:\n    result = 7\nresult\n",
        &[],
        "42\n",
        None,
    );
    compare(
        "fn main() -> i64:\n    if true:\n        return 42\n    else:\n        return 7\n    false\n",
        &[],
        "42\n",
        None,
    );
    let source = include_str!("../../cgx2-cli/examples/conditionals.cg");
    {
        compare(source, &[], "42\n", None);
        for flag in ["true", "false"] {
            let source = format!(
                "fn main() -> i64:\n    if {flag}:\n        return 1 / 0\n    else:\n        return 9223372036854775807 + 1\n"
            );
            compare(
                &source,
                &[],
                "",
                Some(if flag == "true" {
                    "division by zero"
                } else {
                    "i64 arithmetic overflow"
                }),
            );
        }
        compare(
            "record R:\n    n: i64\n    flag: Bool\nfn pick(r: R) -> R:\n    if r.flag:\n        return r\n    else:\n        return R(n: 7, flag: true)\nfn main() -> i64:\n    let r = pick(R(n: 42, flag: true))\n    r.n\n",
            &[],
            "42\n",
            None,
        );
    }
}
