use cgx2_cli::{run_source, Value};
use cgx2_emit_python::emit_python;
use cgx2_sema::{check_module, collect_public_signatures};
use cgx2_syntax::parse_module;
use std::process::Command;

fn checked_python(source: &str) -> String {
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    emit_python(&module, &typed).unwrap()
}

fn run_python(source: &str) -> std::process::Output {
    let python = checked_python(source);
    let path = std::env::temp_dir().join(format!(
        "cgx2-record-{}-{}.py",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, python).unwrap();
    let output = Command::new("python3").arg(&path).output().unwrap();
    std::fs::remove_file(path).unwrap();
    output
}

#[test]
fn reordered_fields_and_record_function_result_match_run() {
    let source = include_str!("fixtures/record.cg");
    let python = checked_python(source);
    assert!(python.contains("class Point:"));
    assert!(python.contains("__slots__ = (\"x\", \"live\",)"));
    assert!(python.contains("Point(live="));
    assert!(python.contains(", x="));
    let cli = run_source(source, &[]).unwrap();
    let output = run_python(source);
    assert_eq!(
        cli.value.to_string(),
        "Point(x: -9223372036854775808, live: true)"
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, format!("{}\n", cli.value).as_bytes());
}

#[test]
fn field_reads_and_shadowed_loop_binding_match_run() {
    let source = "record Point:\n    x: i64\nfn get(p: Point) -> i64:\n    p.x\nfn main() -> i64:\n    let p = Point(x: 7)\n    for p in range(0, 1):\n        p\n    get(p) + p.x\n";
    let cli = run_source(source, &[]).unwrap();
    let output = run_python(source);
    assert_eq!(cli.value, Value::I64(14));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"14\n");
}

#[test]
fn generated_record_instances_are_immutable() {
    let source = "record Point:\n    x: i64\nfn main() -> i64:\n    1\n";
    let python = checked_python(source);
    let probe = format!(
        "ns = {{'__name__': 'cgx2_probe'}}\nexec({python:?}, ns)\np = ns['Point'](x=1)\ntry:\n    p.x = 2\nexcept AttributeError:\n    pass\nelse:\n    raise AssertionError('record field changed')\ntry:\n    del p.x\nexcept AttributeError:\n    pass\nelse:\n    raise AssertionError('record field deleted')\n"
    );
    let output = Command::new("python3")
        .arg("-c")
        .arg(probe)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn record_field_names_do_not_collide_with_python_receiver_or_slots() {
    let source = "record Point:\n    self: i64\n    __slots__: Bool\n    _cgx2_self: i64\nfn main() -> Point:\n    Point(self: 7, __slots__: true, _cgx2_self: 9)\n";
    let cli = run_source(source, &[]).unwrap();
    let output = run_python(source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, format!("{}\n", cli.value).as_bytes());
}

#[test]
fn constructor_preserves_first_checked_arithmetic_error() {
    for (fields, code) in [
        ("y: 1 / 0, x: 9223372036854775807 + 1", "CGR006"),
        ("x: 9223372036854775807 + 1, y: 1 / 0", "CGR005"),
    ] {
        let source = format!(
            "record Pair:\n    x: i64\n    y: i64\nfn main() -> Pair:\n    Pair({fields})\n"
        );
        let cli = run_source(&source, &[]).unwrap_err();
        let output = run_python(&source);
        assert_eq!(cli.code, code);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(code));
    }
}

#[test]
fn invalid_record_fields_stop_before_emission() {
    let header = "record Point:\n    x: i64\n    live: Bool\nfn main() -> Point:\n    ";
    for (body, code) in [
        ("Point(x: 1)", "CGT019"),
        ("Point(x: 1, live: true, extra: 3)", "CGT019"),
    ] {
        let source = format!("{header}{body}\n");
        let module = parse_module(&source).unwrap();
        assert!(check_module(&module)
            .unwrap_err()
            .iter()
            .any(|error| error.code == code));
    }
    let source = format!("{header}Point(x: 1, x: 2, live: true)\n");
    assert!(parse_module(&source)
        .unwrap_err()
        .iter()
        .any(|error| error.code == "CGS106"));
}

#[test]
fn mutable_record_binding_is_rejected() {
    let source =
        "record Point:\n    x: i64\nfn main() -> i64:\n    let mut p = Point(x: 1)\n    p.x\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let errors = emit_python(&module, &typed).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("mutable record")));
}

#[test]
fn unsupported_record_shapes_never_reach_python_emission() {
    for source in [
        "record Inner:\n    x: i64\nrecord Outer:\n    inner: Inner\nfn main() -> i64:\n    1\n",
        "record Bag:\n    values: List<i64>\nfn main() -> i64:\n    1\n",
        "record Pointer:\n    value: owned<i64>\nfn main() -> i64:\n    1\n",
    ] {
        let module = parse_module(source).unwrap();
        assert!(check_module(&module).is_err(), "{source}");
    }
    let public =
        parse_module("record Point:\n    x: i64\npub fn id(p: Point) -> Point:\n    p\n").unwrap();
    assert!(collect_public_signatures(&public).is_err());
}
