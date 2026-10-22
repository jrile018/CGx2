use cgx2_cli::{run_source, Value};
use cgx2_emit_python::emit_python;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;
use std::process::Command;

fn run_python(source: &str, args: &[&str]) -> std::process::Output {
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let python = emit_python(&module, &typed).unwrap();
    let path = std::env::temp_dir().join(format!(
        "cgx2-differential-{}-{}.py",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, python).unwrap();
    let output = Command::new("python3")
        .arg(&path)
        .args(args)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    output
}

fn compare(source: &str, args: &[&str], expected_stdout: &str, success: bool) {
    let output = run_python(source, args);
    let rust_args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let cli_result = run_source(source, &rust_args);
    let cli_stdout = match &cli_result {
        Ok(result) if result.value != Value::Unit => format!("{}\n", result.value),
        _ => String::new(),
    };
    assert_eq!(cli_result.is_ok(), success);
    assert_eq!(output.status.success(), success);
    assert_eq!(cli_stdout, expected_stdout);
    assert_eq!(output.stdout, expected_stdout.as_bytes());
    if !success {
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn scalar_contracts_match_cli_outcome_order_and_result() {
    let cases = [
        ("fn main() -> i64 effects() caps():\n    requires true\n    ensures result == 7\n    7\n", "7\n", None),
        ("fn main() -> i64:\n    requires false\n    9223372036854775807 + 1\n", "", Some("CGR015")),
        ("fn main() -> i64:\n    ensures result == 8\n    7\n", "", Some("CGR016")),
        ("fn main() -> i64:\n    requires 9223372036854775807 + 1 > 0\n    7\n", "", Some("CGR005")),
        ("fn main() -> i64:\n    ensures result + 1 > result\n    9223372036854775807\n", "", Some("CGR005")),
        ("fn main() -> i64:\n    requires false\n    ensures result == 8\n    7\n", "", Some("CGR015")),
        ("fn main() -> i64:\n    ensures result == 7\n    let result = 6\n    return result + 1\n", "7\n", None),
        ("fn positive(x: i64) -> Bool:\n    x > 0\nfn main() -> i64:\n    requires positive(3)\n    ensures result > 0\n    7\n", "7\n", None),
        ("fn main() -> i64 effects(ReadArgs) caps(ArgsRead):\n    arg_count()\n", "0\n", None),
    ];
    for (source, stdout, code) in cases {
        let output = run_python(source, &[]);
        let cli = run_source(source, &[]);
        assert_eq!(output.stdout, stdout.as_bytes(), "{source}");
        match code {
            None => {
                assert!(cli.is_ok(), "{source}");
                assert!(output.status.success(), "{source}");
            }
            Some(code) => {
                assert_eq!(cli.unwrap_err().code, code, "{source}");
                assert!(!output.status.success(), "{source}");
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(stderr.contains(code), "{source}: {stderr}");
                if matches!(code, "CGR015" | "CGR016") {
                    let condition = if code == "CGR015" {
                        "false"
                    } else {
                        "result == 8"
                    };
                    let start = source.find(condition).unwrap();
                    assert!(
                        stderr.contains(&format!("bytes [{start}, {})", start + condition.len())),
                        "{source}: {stderr}"
                    );
                }
            }
        }
    }
}

#[test]
fn scripts_match_cli() {
    compare(include_str!("fixtures/script.cg"), &[], "21\n", true);
}

#[test]
fn function_modules_match_cli() {
    compare(include_str!("fixtures/functions.cg"), &[], "13\n", true);
}

#[test]
fn argument_indexing_matches_cli() {
    compare(
        include_str!("fixtures/arguments.cg"),
        &["hello 世界"],
        "hello 世界\n",
        true,
    );
}

#[test]
fn checked_errors_fail_on_both_targets() {
    compare(include_str!("fixtures/checked_error.cg"), &[], "", false);
}
