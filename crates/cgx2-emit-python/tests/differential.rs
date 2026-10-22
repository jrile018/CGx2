use cgx2_cli::{run_source, Value};
use cgx2_emit_python::emit_python;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;
use std::process::Command;

fn compare(source: &str, args: &[&str], expected_stdout: &str, success: bool) {
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
