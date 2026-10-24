use cgx2_sema::ordinary_owned::{self, model, Evidence};

// Exhaustive matches intentionally have no wildcard: adding schema-2 cases to
// the public schema-1 enums would break these existing caller shapes.
fn old_type(t: model::Ty) -> bool {
    match t {
        model::Ty::I64
        | model::Ty::Bool
        | model::Ty::Unit
        | model::Ty::Record(_)
        | model::Ty::Owned(_)
        | model::Ty::Shared(_)
        | model::Ty::Exclusive(_) => true,
    }
}
fn old_expression(e: &model::ExpressionKind) -> bool {
    match e {
        model::ExpressionKind::I64(_)
        | model::ExpressionKind::Bool(_)
        | model::ExpressionKind::Unit
        | model::ExpressionKind::Local(_)
        | model::ExpressionKind::Field { .. }
        | model::ExpressionKind::Record { .. }
        | model::ExpressionKind::Owned(_)
        | model::ExpressionKind::Borrow { .. }
        | model::ExpressionKind::Call { .. }
        | model::ExpressionKind::Binary { .. } => true,
    }
}
fn old_statement(s: &model::StatementKind) -> bool {
    match s {
        model::StatementKind::Let { .. }
        | model::StatementKind::Set { .. }
        | model::StatementKind::Evaluate(_)
        | model::StatementKind::Return(_)
        | model::StatementKind::If { .. } => true,
    }
}
#[test]
fn old_exhaustive_model_and_evidence_literal_still_compile_and_execute() {
    let source = "record R:\n    x: i64\nfn main() -> i64:\n    let p = owned(R(x: 42))\n    p.x\n";
    let old = ordinary_owned::compile_source(source).unwrap();
    let model: &model::Model = old.model();
    assert!(old_type(model.functions[0].result));
    assert!(old_statement(&model.functions[0].body[0].kind));
    let model::StatementKind::Return(value) = &model.functions[0].body[1].kind else {
        panic!("expected return");
    };
    assert!(old_expression(&value.kind));
    let zero = Evidence {
        allocations: 0,
        releases: 0,
        live_bytes: 0,
        allocated_bytes: 0,
        released_bytes: 0,
        peak_live_bytes: 0,
        reads: 0,
        writes: 0,
    };
    assert_eq!(zero, Evidence::default());
    let upgraded = old.upgrade_v2().unwrap();
    let before = old.execute().unwrap();
    let after = upgraded.execute().unwrap();
    assert_eq!(before.result, after.result);
    assert_eq!(before.evidence.json(), after.evidence.json());
}
#[test]
fn new_shapes_require_explicit_v2_entry_without_placeholder_v1_model() {
    let source = include_str!("../../cgx2-syntax/owned-result-evidence/fixtures/recover.cg");
    assert_eq!(
        ordinary_owned::compile_source(source).unwrap_err()[0].code,
        "CGT081"
    );
    let new = ordinary_owned::compile_source_v2(source).unwrap();
    assert_eq!(new.schema_version(), 2);
    assert!(new.ownership_functions().is_empty());
    assert!(!new.ownership_functions_v2().is_empty());
    assert_eq!(
        new.execute().unwrap().result,
        Ok(ordinary_owned::Scalar::I64(42))
    );
}

#[test]
fn copied_error_signatures_also_require_v2_even_when_unused() {
    let source = "record R:\n    x: i64\nfn owner(p: owned<R>) -> owned<R>:\n    p\nfn error(e: AllocationError) -> AllocationError:\n    e\nfn main() -> i64:\n    42\n";
    let new = ordinary_owned::compile_source_v2(source).unwrap();
    assert_eq!(new.schema_version(), 2);
    assert!(new.ownership_functions().is_empty());
    assert!(new.summaries().is_empty());
    assert_eq!(
        new.execute().unwrap().result,
        Ok(ordinary_owned::Scalar::I64(42))
    );
    assert!(ordinary_owned::compile_source(source).is_err());
}
