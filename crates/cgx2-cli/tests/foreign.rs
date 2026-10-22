use cgx2_cli::{check_source, run_checked_source, run_source, Value};
use std::{fs, process::Command};

fn source(target: &str, result: &str, argument: &str, extra: &str) -> String {
    format!("foreign python pub fn host(x: i64) -> {result} effects(Foreign) caps(ForeignCall) = {target} runtime \"local:cpython\" ownership copied\n\nfn main() -> i64 effects(Foreign) caps(ForeignCall):\n{extra}    host({argument})\n")
}

#[test]
fn declared_stdlib_call_returns_checked_scalar() {
    let code = source("math.isqrt", "i64", "81", "");
    let (result, _) = run_checked_source(&code, &[]).unwrap();
    assert_eq!(result.value, Value::I64(9));

    let string = "foreign python fn text(x: i64) -> String effects(Foreign) caps(ForeignCall) = builtins.str runtime \"local:cpython\" ownership copied\n\nfn main() -> String effects(Foreign) caps(ForeignCall):\n    text(12)\n";
    assert_eq!(
        run_checked_source(string, &[]).unwrap().0.value,
        Value::String("12".into())
    );
    let boolean = "foreign python fn truth(x: i64) -> Bool effects(Foreign) caps(ForeignCall) = builtins.bool runtime \"local:cpython\" ownership copied\n\nfn main() -> Bool effects(Foreign) caps(ForeignCall):\n    truth(1)\n";
    assert_eq!(
        run_checked_source(boolean, &[]).unwrap().0.value,
        Value::Bool(true)
    );
}

#[test]
fn low_level_source_runner_checks_foreign_declarations_before_execution() {
    let missing_effects = "foreign python fn host(x: i64) -> i64 = math.isqrt runtime \"local:cpython\" ownership copied\n\nfn main() -> i64:\n    host(81)\n";
    let error = run_source(missing_effects, &[]).unwrap_err();
    assert!(matches!(error.code, "CGT012" | "CGT013" | "CGT016"));

    let missing_return = "foreign python fn host(x: i64) = math.isqrt runtime \"local:cpython\" ownership copied\n\nfn main() -> i64:\n    host(81)\n";
    assert!(run_source(missing_return, &[]).is_err());
}

#[test]
fn static_foreign_gate_rejects_type_arity_effect_capability_and_torch() {
    let valid = source("math.isqrt", "i64", "81", "");
    let cases = [
        (valid.replace("x: i64", "x: List<i64>"), "CGT016"),
        (valid.replace("host(81)", "host(81, 2)"), "CGT005"),
        (valid.replace("host(81)", "host(true)"), "CGT003"),
        (
            valid.replace(
                "fn main() -> i64 effects(Foreign)",
                "fn main() -> i64 effects()",
            ),
            "CGT012",
        ),
        (
            valid.replace(
                "fn main() -> i64 effects(Foreign) caps(ForeignCall)",
                "fn main() -> i64 effects(Foreign) caps()",
            ),
            "CGT013",
        ),
        (valid.replace("math.isqrt", "torch.isqrt"), "CGT016"),
        (valid.replace("math.isqrt", "builtins.eval"), "CGT016"),
        (valid.replace("math.isqrt", "os.system"), "CGT016"),
        (valid.replace("math.isqrt", "builtins.open"), "CGT016"),
    ];
    for (source, code) in cases {
        let errors = check_source(&source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == code),
            "{source}\n{errors:?}"
        );
    }
}

#[test]
fn python_errors_and_wrong_return_keep_call_span() {
    for (target, result, argument, code) in [
        ("math.missing_cgx2_callable", "i64", "4", "CGR019"),
        ("math.isqrt", "i64", "0 - 1", "CGR020"),
        ("math.sqrt", "i64", "4", "CGR021"),
    ] {
        let source = source(target, result, argument, "");
        let errors = run_checked_source(&source, &[]).unwrap_err();
        let error = &errors[0];
        assert_eq!(error.code, code, "{source}\n{errors:?}");
        let span = error.span.unwrap();
        assert_eq!(&source[span.start..span.end], format!("host({argument})"));
    }
}

#[test]
fn unknown_module_and_foreign_main_fail_static_check() {
    let unknown = source("missing_cgx2_module.isqrt", "i64", "4", "");
    assert!(check_source(&unknown)
        .unwrap_err()
        .iter()
        .any(|error| error.code == "CGT016"));

    let foreign_main = "foreign python fn main() -> i64 effects(Foreign) caps(ForeignCall) = math.isqrt runtime \"local:cpython\" ownership copied\n";
    assert!(check_source(foreign_main)
        .unwrap_err()
        .iter()
        .any(|error| error.code == "CGT016"));
    assert!(run_source(foreign_main, &[]).is_err());
}

#[test]
fn requires_precedes_python_and_ensures_follows_typed_return() {
    let code = source("math.isqrt", "i64", "0 - 1", "    requires false\n");
    let errors = run_checked_source(&code, &[]).unwrap_err();
    assert_eq!(errors[0].code, "CGR015");
    let code = source("math.isqrt", "i64", "0 - 1", "    ensures result == 1\n");
    let errors = run_checked_source(&code, &[]).unwrap_err();
    assert_eq!(errors[0].code, "CGR020");
    let code = source("math.isqrt", "i64", "4", "    ensures result == 3\n");
    let errors = run_checked_source(&code, &[]).unwrap_err();
    assert_eq!(errors[0].code, "CGR016");
}

#[test]
fn cli_json_reports_utf8_foreign_call_span() {
    let code = format!(
        "module café\n\n{}",
        source("math.isqrt", "i64", "0 - 1", "")
    );
    let path = std::env::temp_dir().join(format!("cgx2-foreign-{}.cg", std::process::id()));
    fs::write(&path, &code).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["run", path.to_str().unwrap(), "--diagnostic-format=json"])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let start = code.find("host(0 - 1)").unwrap();
    assert!(stderr.contains("\"code\":\"CGR020\""), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "\"span\":{{\"start\":{start},\"end\":{}",
            start + "host(0 - 1)".len()
        )),
        "{stderr}"
    );
}
