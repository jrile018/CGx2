use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

use cgx2_cli::check_source;
use cgx2_syntax::Span;

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct SourceFile(PathBuf);

impl SourceFile {
    fn new(source: &str) -> Self {
        Self::named(source, "source")
    }

    fn named(source: &str, name: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "cgx2-check-test-{}-{id}-{name}.cg",
            std::process::id()
        ));
        fs::write(&path, source).unwrap();
        Self(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for SourceFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(args)
        .output()
        .unwrap()
}

fn error_span(source: &str, code: &str, expected: &str) {
    let errors = check_source(source).unwrap_err();
    let error = errors.iter().find(|error| error.code == code).unwrap();
    assert_eq!(&source[error.span.start..error.span.end], expected);
}

#[test]
fn check_source_accepts_scalar_script_and_module() {
    let script = check_source("let x = 1\nx + 2\n").unwrap();
    assert!(!script.expressions.iter().collect::<Vec<_>>().is_empty());
    let module = check_source("module demo\nfn main() -> i64:\n    7\n").unwrap();
    assert_eq!(module.functions["main"].return_type.to_string(), "i64");
}

#[test]
fn check_source_preserves_codes_and_byte_spans() {
    error_span("missing\n", "CGT001", "missing");
    error_span("let x: i64 = true\n", "CGT003", "true");
    error_span("let x = 1\nx = 2\n", "CGT004", "x");
    error_span("let x =\n", "CGS103", "\n");
    let source = "let café = 1\nmissing\n";
    let errors = check_source(source).unwrap_err();
    assert_eq!(errors[0].span, Span { start: 14, end: 21 });
}

#[test]
fn check_and_run_gate_static_errors() {
    let valid = SourceFile::new("let x = 1\nx + 2\n");
    let output = invoke(&["check", valid.path()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());

    let invalid = SourceFile::new("let x: i64 = true\nx\n");
    for command in ["check", "run"] {
        let output = invoke(&[command, invalid.path()]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8(output.stderr).unwrap().contains("CGT003"));
    }
}

#[test]
fn help_states_check_scope() {
    let output = invoke(&["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("check FILE.cg"));
    assert!(help.contains("bounded memory/ownership script subset"));
}

#[test]
fn memory_check_fmt_and_run_share_checked_contract() {
    let source = "let r = memory.region(mode: manual, limit: 4096)\nno_gc(budget: 256):\n    let x = memory.alloc_bytes(r, 64)\nmemory.collect(r)\n";
    let file = SourceFile::new(source);
    assert!(invoke(&["check", file.path()]).status.success());
    let formatted = invoke(&["fmt", file.path()]);
    assert!(formatted.status.success());
    assert_eq!(formatted.stdout, source.as_bytes());
    let run = invoke(&["run", file.path(), "--diagnostic-format=json"]);
    assert!(run.status.success());
    assert!(run.stdout.is_empty());

    let invalid_source =
        "let r = memory.region(mode: manual, limit: 4)\nno_gc(budget: 2):\n    memory.collect(r)\n";
    let invalid = SourceFile::new(invalid_source);
    let errors = json_errors(invoke(&[
        "check",
        invalid.path(),
        "--diagnostic-format=json",
    ]));
    assert_eq!(errors[0].code, "CGO004");
    let start = invalid_source.find("memory.collect(r)").unwrap();
    assert_eq!(
        errors[0].span,
        Some(Span {
            start,
            end: start + "memory.collect(r)".len()
        })
    );
}

#[test]
fn run_preserves_arguments_and_reports_unsupported_for() {
    let args = SourceFile::new("arg(0)\n");
    for command in [Some("run"), None] {
        let mut argv = Vec::new();
        if let Some(command) = command {
            argv.push(command);
        }
        argv.extend([args.path(), "--", "hello 世界"]);
        let output = invoke(&argv);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, "hello 世界\n".as_bytes());
    }
    let output = invoke(&["run", args.path(), "--", "--diagnostic-format=json"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"--diagnostic-format=json\n");

    let count = SourceFile::new("arg_count()\n");
    let output = invoke(&["run", count.path(), "--", "hello 世界", "two words"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"2\n");

    let for_loop = SourceFile::new("fn unused(xs: List<i64>) -> i64:\n    for x in xs:\n        x\n    0\nfn main() -> i64:\n    1\n");
    // The static checker accepts this function; execution rejects the preview's for syntax.
    assert!(invoke(&["check", for_loop.path()]).status.success());
    let output = invoke(&["run", for_loop.path()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr).unwrap().contains("CGR009"));
}

#[derive(Debug, PartialEq, Eq)]
struct JsonDiagnostic {
    code: String,
    message: String,
    file: String,
    span: Option<Span>,
}

// Decode the exact published JSON Lines shape, including escaped strings and byte offsets.
fn decode_json_line(line: &str) -> JsonDiagnostic {
    struct Cursor<'a> {
        input: &'a str,
        position: usize,
    }

    impl Cursor<'_> {
        fn literal(&mut self, value: &str) {
            assert!(
                self.input[self.position..].starts_with(value),
                "{}",
                self.input
            );
            self.position += value.len();
        }

        fn string(&mut self) -> String {
            self.literal("\"");
            let mut decoded = String::new();
            loop {
                let character = self.input[self.position..].chars().next().unwrap();
                self.position += character.len_utf8();
                match character {
                    '"' => break,
                    '\\' => {
                        let escaped = self.input[self.position..].chars().next().unwrap();
                        self.position += escaped.len_utf8();
                        match escaped {
                            '"' => decoded.push('"'),
                            '\\' => decoded.push('\\'),
                            '/' => decoded.push('/'),
                            'n' => decoded.push('\n'),
                            'r' => decoded.push('\r'),
                            't' => decoded.push('\t'),
                            'b' => decoded.push('\x08'),
                            'f' => decoded.push('\x0c'),
                            'u' => {
                                let digits = &self.input[self.position..self.position + 4];
                                let value = u32::from_str_radix(digits, 16).unwrap();
                                decoded.push(char::from_u32(value).unwrap());
                                self.position += 4;
                            }
                            _ => panic!("invalid JSON escape: {escaped}"),
                        }
                    }
                    character if character >= ' ' => decoded.push(character),
                    _ => panic!("unescaped JSON control character"),
                }
            }
            decoded
        }

        fn number(&mut self) -> usize {
            let start = self.position;
            while self
                .input
                .as_bytes()
                .get(self.position)
                .is_some_and(u8::is_ascii_digit)
            {
                self.position += 1;
            }
            self.input[start..self.position].parse().unwrap()
        }
    }

    let mut cursor = Cursor {
        input: line,
        position: 0,
    };
    cursor.literal("{\"code\":");
    let code = cursor.string();
    cursor.literal(",\"severity\":\"error\",\"message\":");
    let message = cursor.string();
    cursor.literal(",\"file\":");
    let file = cursor.string();
    cursor.literal(",\"span\":");
    let span = if cursor.input[cursor.position..].starts_with("null") {
        cursor.literal("null");
        None
    } else {
        cursor.literal("{\"start\":");
        let start = cursor.number();
        cursor.literal(",\"end\":");
        let end = cursor.number();
        cursor.literal("}");
        Some(Span { start, end })
    };
    cursor.literal("}");
    assert_eq!(cursor.position, line.len());
    JsonDiagnostic {
        code,
        message,
        file,
        span,
    }
}

fn json_errors(output: Output) -> Vec<JsonDiagnostic> {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    stderr.lines().map(decode_json_line).collect()
}

#[test]
fn json_lines_report_every_type_error_with_byte_spans() {
    let file = SourceFile::new("missing\nother\n");
    for argv in [
        vec!["--diagnostic-format=json", "check", file.path()],
        vec!["check", file.path(), "--diagnostic-format=json"],
        vec!["run", file.path(), "--diagnostic-format=json"],
    ] {
        let errors = json_errors(invoke(&argv));
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert_eq!(errors[0].code, "CGT001");
        assert_eq!(errors[0].file, file.path());
        assert_eq!(errors[0].span, Some(Span { start: 0, end: 7 }));
        assert_eq!(errors[1].code, "CGT001");
        assert_eq!(errors[1].span, Some(Span { start: 8, end: 13 }));
    }
    let unicode = SourceFile::new("let café = 1\nmissing\n");
    let errors = json_errors(invoke(&[
        "check",
        unicode.path(),
        "--diagnostic-format=json",
    ]));
    assert_eq!(errors[0].span, Some(Span { start: 14, end: 21 }));
}

#[test]
fn json_escapes_file_and_message_and_preserves_code_origins() {
    let source = "\\\n";
    let syntax = SourceFile::named(source, "quote\"slash\\");
    let raw = invoke(&["check", syntax.path(), "--diagnostic-format=json"]);
    let output = String::from_utf8(raw.stderr.clone()).unwrap();
    assert!(output.contains("\\\"slash\\\\"), "{output}");
    let errors = json_errors(raw);
    assert_eq!(errors[0].code, "CGS004");
    assert_eq!(errors[0].file, syntax.path());
    assert_eq!(
        errors[0].message,
        check_source(source).unwrap_err()[0].message
    );
    assert!(errors[0].message.contains('\\'));

    let runtime = SourceFile::new("1 / 0\n");
    let errors = json_errors(invoke(&["run", runtime.path(), "--diagnostic-format=json"]));
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "CGR006");
    assert_eq!(errors[0].file, runtime.path());
    assert!(errors[0].span.is_some());
}
