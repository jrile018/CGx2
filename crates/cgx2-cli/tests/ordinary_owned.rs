use cgx2_cli::{run_checked_source, run_source, Value};
const SOURCE: &str =
    include_str!("../../cgx2-syntax/ordinary-owned-evidence/fixtures/functions.cg");

#[test]
fn both_execution_entries_select_ownership_before_copied_lookup() {
    assert_eq!(run_source(SOURCE, &[]).unwrap().value, Value::I64(37));
    assert_eq!(
        run_checked_source(SOURCE, &[]).unwrap().0.value,
        Value::I64(37)
    );
    let moved =
        include_str!("../../cgx2-syntax/ordinary-owned-evidence/fixtures/use-after-move.cg");
    let errors = std::iter::once(run_source(moved, &[]).unwrap_err())
        .chain(run_checked_source(moved, &[]).unwrap_err());
    for error in errors {
        assert_eq!(error.code, "CGO101");
        let span = error.span.unwrap();
        assert_eq!(&moved[span.start..span.end], "p.x");
    }
}
#[test]
fn copied_record_results_cross_an_owned_call_without_becoming_owners() {
    let source =
        include_str!("../../cgx2-syntax/ordinary-owned-evidence/fixtures/copied-record.cg");
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value.to_string(),
        "R(x: 3, yes: true)"
    );
}
