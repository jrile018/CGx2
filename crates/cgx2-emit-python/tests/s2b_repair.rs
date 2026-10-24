//! Execute the repaired finite emitters, including standalone Rust 2021.
use cgx2_cli::run_source;
use cgx2_emit_python::emit_python;
use cgx2_emit_rust::emit_rust;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn parity(source: &str, expected: &str) {
    let interpreted = run_source(source, &[]).unwrap();
    assert_eq!(format!("{}\n", interpreted.value), expected);
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let rust = emit_rust(&module, &typed).unwrap();
    assert_eq!(rust, emit_rust(&module, &typed).unwrap());
    let python = emit_python(&module, &typed).unwrap();
    assert_eq!(python, emit_python(&module, &typed).unwrap());
    let dir = std::env::temp_dir().join(format!(
        "cgx2-s2b-repair-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.rs"), rust).unwrap();
    fs::write(dir.join("main.py"), python).unwrap();
    for optimized in [false, true] {
        let mut rustc = Command::new("rustc");
        rustc
            .current_dir(&dir)
            .args(["--edition=2021", "main.rs", "-o", "native"]);
        if optimized {
            rustc.arg("-O");
        }
        let compiled = rustc.output().unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let output = Command::new(dir.join("native"))
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
    let output = Command::new("python3")
        .arg("main.py")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(output.stderr.is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_specialization_types_include_nested_data_and_contracts_in_both_orders() {
    let source = include_str!("fixtures/s2b-repair/local-types.cg");
    parity(source, "2\n");
    parity(
        &source.replace("size(1) + size(true)", "size(true) + size(1)"),
        "2\n",
    );
    // Contract-only generic calls must also discover their concrete local types.
    parity(
        &source.replace(
            "    size(1) + size(true)",
            "    requires size(1) == 1\n    requires size(true) == 1\n    2",
        ),
        "2\n",
    );
    parity("fn bounded<T>(value: i64) -> i64:\n    requires length([get([Option<T>.None()], 0)]) == 1\n    ensures length([get([Option<T>.None()], 0)]) == result\n    length([value])\nfn main() -> i64:\n    bounded<i64>(1) + bounded<Bool>(1)\n", "2\n");
}

#[test]
fn finite_strings_use_cgx2_escape_and_unicode_semantics() {
    parity(
        include_str!("fixtures/s2b-repair/strings.cg"),
        "line\n\r\t\\\"é界💡\n",
    );
    // A literal backslash followed by x is legal, and must remain literal.
    parity("concat(\"\\\\x41\", \"\\\\u0041\")\n", "\\x41\\u0041\n");
}

#[test]
fn invalid_escapes_reject_with_literal_spans_before_emission() {
    for raw in [
        r#""\x41""#,
        r#""\u0041""#,
        r#""\U00000041""#,
        r#""\101""#,
        r#""\q""#,
        r#""\b""#,
    ] {
        let cases = [
            format!("concat({raw}, \"\")\n"),
            format!("fn bad<T>(value: T) -> i64:\n    let nested = get([[{raw}]], 0)\n    length([value])\nfn main() -> i64:\n    bad(1)\n"),
            format!("fn bad<T>(value: i64) -> i64:\n    requires length({raw}) == 1\n    length([value])\nfn main() -> i64:\n    bad<Bool>(1)\n"),
            format!("fn bad<T>(value: i64) -> i64:\n    ensures length({raw}) == result\n    length([value])\nfn main() -> i64:\n    bad<Bool>(1)\n"),
            format!("record Box<T>:\n    value: T\nfn main() -> i64:\n    let b = Box(value: [{raw}])\n    length(b.value)\n"),
            format!("variant Text:\n    Value(String)\nfn main() -> i64:\n    let v = Text.Value({raw})\n    1\n"),
        ];
        for source in cases {
            let module = parse_module(&source).unwrap();
            let typed = check_module(&module).unwrap();
            let rust_errors = emit_rust(&module, &typed).unwrap_err();
            let python_errors = emit_python(&module, &typed).unwrap_err();
            assert_eq!(rust_errors[0].code, "CGRS003", "{source}");
            assert_eq!(python_errors[0].code, "CGE003", "{source}");
            for span in [rust_errors[0].span, python_errors[0].span] {
                assert_eq!(&source[span.start..span.end], raw);
            }
        }
    }
}

#[test]
fn invalid_imported_literal_retains_file_span_and_existing_package_output() {
    let dir = std::env::temp_dir().join(format!("cgx2-s2b-package-{}", std::process::id()));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("cgx2.toml"), "schema_version = 1\nname = \"repair\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\", \"src/text.cg\"]\n").unwrap();
    fs::write(
        dir.join("src/main.cg"),
        "module repair.main\nimport repair.text\nfn main() -> String:\n    repair.text.bad(1)\n",
    )
    .unwrap();
    let source =
        "module repair.text\npub fn bad<T>(value: T) -> String:\n    concat(\"\\x41\", \"\")\n";
    fs::write(dir.join("src/text.cg"), source).unwrap();
    let package = cgx2_package::load_checked_package(&dir).unwrap();
    let rust = cgx2_emit_rust::emit_package_rust(&package).unwrap_err();
    assert_eq!(rust[0].diagnostic.code, "CGRS003");
    assert_eq!(rust[0].file, std::path::Path::new("src/text.cg"));
    let span = rust[0].diagnostic.span;
    assert_eq!(&source[span.start..span.end], r#""\x41""#);
    let output = dir.join("output");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("sentinel"), "preserve").unwrap();
    let python = cgx2_emit_python::write_package_python(&package, &output).unwrap_err();
    assert_eq!(python[0].code, "CGE003");
    assert_eq!(python[0].file, std::path::Path::new("src/text.cg"));
    assert_eq!(python[0].span, Some(span));
    assert_eq!(
        fs::read_to_string(output.join("sentinel")).unwrap(),
        "preserve"
    );
    assert_eq!(fs::read_dir(output).unwrap().count(), 1);
    fs::remove_dir_all(dir).unwrap();
}
