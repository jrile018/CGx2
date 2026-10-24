use cgx2_cli::{run_checked_source, run_native_package, Value};
use cgx2_syntax::parse_module;
use std::path::Path;

const FIXTURE: &str = include_str!("../../cgx2-syntax/s2a-evidence/fixtures/variants.cg");

#[test]
fn nested_concrete_types_and_selected_arms_execute() {
    assert_eq!(
        run_checked_source(FIXTURE, &[]).unwrap().0.value,
        Value::I64(42)
    );
    let error = run_checked_source(
        &FIXTURE.replace("Reply.Value(41, true)", "Reply.Value(0, true)"),
        &[],
    )
    .unwrap_err();
    assert_eq!(error[0].code, "CGR015");
    let error =
        run_checked_source(&FIXTURE.replace("x + 1", "9223372036854775807 + 1"), &[]).unwrap_err();
    assert_eq!(error[0].code, "CGR005");
    let error = run_checked_source(
        &FIXTURE.replace("ensures result > 0", "ensures result < 0"),
        &[],
    )
    .unwrap_err();
    assert_eq!(error[0].code, "CGR016");
}

#[test]
fn imported_collisions_keep_nominal_identity_and_api_is_deterministic() {
    let root = Path::new("../cgx2-syntax/s2a-evidence/fixtures/package");
    assert_eq!(run_native_package(root, &[]).unwrap().value, Value::I64(42));
    let package = cgx2_package::load_checked_package(root).unwrap();
    let json = package.index().render_json();
    assert_eq!(
        json,
        cgx2_package::load_checked_package(root)
            .unwrap()
            .index()
            .render_json()
    );
    assert!(json.contains("app.left.Reply"));
    assert!(json.contains("app.right.Reply"));
    assert!(json.contains("payloads"));
    assert!(!json.contains("Secret"));
}

#[test]
fn result_error_and_option_none_are_typed_and_runnable() {
    let source = "fn choose(x: Result<i64, Bool>) -> Option<i64>:\n    match x:\n        Result<i64, Bool>.Ok(value):\n            Option<i64>.Some(value)\n        Result<i64, Bool>.Err(_):\n            Option<i64>.None()\nfn main() -> i64:\n    match choose(Result<i64, Bool>.Err(true)):\n        Option<i64>.Some(value):\n            value\n        Option<i64>.None:\n            42\n";
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value,
        Value::I64(42)
    );
}

#[test]
fn pattern_scope_shadows_outer_binding_without_leaking() {
    let source = "variant Item:\n    Value(i64)\nfn main() -> i64:\n    let x = 7\n    match Item.Value(99):\n        Item.Value(x):\n            x\n    x\n";
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value,
        Value::I64(7)
    );
    let error = run_checked_source(&source.replace("    let x = 7\n", ""), &[]).unwrap_err();
    assert!(error.iter().any(|e| e.code == "CGT001"));
}

#[test]
fn scrutinee_is_evaluated_once_and_unselected_arm_is_not_evaluated() {
    // The interpreter charges each evaluation. Keep this just below its fixed
    // step budget: evaluating make() twice crosses the budget.
    let source = include_str!("../../cgx2-syntax/s2a-evidence/fixtures/evaluation-once.cg");
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value,
        Value::I64(200000)
    );
}

#[test]
fn portable_python_executes_variants() {
    let module = parse_module(FIXTURE).unwrap();
    let typed = cgx2_sema::check_module(&module).unwrap();
    // Exercise the product command so no extra test dependency is needed.
    let path = std::env::temp_dir().join(format!("cgx2-variant-{}.py", std::process::id()));
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args([
            "emit",
            "../cgx2-syntax/s2a-evidence/fixtures/variants.cg",
            "--target",
            "python",
            "--out",
            path.to_str().unwrap(),
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let python = std::process::Command::new("python3")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        python.status.success(),
        "{}",
        String::from_utf8_lossy(&python.stderr)
    );
    std::fs::remove_file(path).unwrap();
    assert!(!typed.expressions.iter().collect::<Vec<_>>().is_empty());
}

#[test]
fn new_contextual_words_preserve_existing_scalar_names() {
    let source="fn match(x: i64) -> i64:\n    x + 1\nfn invariant(x: i64) -> i64:\n    match(x)\nfn decreases(x: i64) -> i64:\n    invariant(x)\nfn main() -> i64:\n    decreases(41)\n";
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value,
        Value::I64(42)
    );
    assert_eq!(
        run_checked_source("let variant = 42\nvariant\n", &[])
            .unwrap()
            .0
            .value,
        Value::I64(42)
    );
}
