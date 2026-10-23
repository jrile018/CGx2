use cgx2_cli::{run_checked_source, Value};

#[test]
fn records_copy_values_and_evaluate_reordered_fields_in_source_order() {
    let source = "record Pair:\n    left: i64\n    right: i64\n    yes: Bool\nfn swap(p: Pair) -> Pair:\n    Pair(yes: p.yes, right: p.left, left: p.right)\nfn main() -> Bool:\n    let p = Pair(right: -9223372036854775808, yes: true, left: 9223372036854775807)\n    let q = swap(p)\n    q.yes\n";
    let (result, report) = run_checked_source(source, &[]).unwrap();
    assert_eq!(result.value, Value::Bool(true));
    assert!(report.is_none());
    let first = source
        .replace("    q.yes\n", "    q.left\n")
        .replace("fn main() -> Bool", "fn main() -> i64");
    assert_eq!(
        run_checked_source(&first, &[]).unwrap().0.value,
        Value::I64(i64::MIN)
    );
}

#[test]
fn first_field_error_follows_source_order() {
    let source = "record Pair:\n    x: i64\n    y: i64\nfn main() -> i64:\n    let p = Pair(y: 1 / 0, x: 9223372036854775807 + 1)\n    p.x\n";
    let error = run_checked_source(source, &[]).unwrap_err().remove(0);
    assert_eq!(error.code, "CGR006");
    let source = source.replace(
        "y: 1 / 0, x: 9223372036854775807 + 1",
        "x: 9223372036854775807 + 1, y: 1 / 0",
    );
    let error = run_checked_source(&source, &[]).unwrap_err().remove(0);
    assert_eq!(error.code, "CGR005");
}
