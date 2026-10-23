use cgx2_verify_core::{
    audit_encoding, audit_encoding_with_source, emit_smtlib, AuditError, Contract, Domain, Expr,
    Parameter, Sort, Value,
};

fn one_parameter(requires: Expr, ensures: Expr) -> Contract {
    Contract {
        parameters: vec![Parameter::new("x", Sort::I64)],
        requires,
        ensures,
    }
}

fn x() -> Expr {
    Expr::Var("x".into())
}

#[test]
fn every_tuple_and_each_named_obligation_are_checked() {
    let contract = Contract {
        parameters: vec![
            Parameter::new("x", Sort::I64),
            Parameter::new("b", Sort::Bool),
        ],
        requires: Expr::And(
            Box::new(Expr::Gt(Box::new(x()), Box::new(Expr::I64(0)))),
            Box::new(Expr::Var("b".into())),
        ),
        ensures: Expr::Or(
            Box::new(Expr::Gt(
                Box::new(Expr::Add(Box::new(x()), Box::new(Expr::I64(1)))),
                Box::new(x()),
            )),
            Box::new(Expr::Not(Box::new(Expr::Var("b".into())))),
        ),
    };
    let script = emit_smtlib(&contract).unwrap();
    let audit = audit_encoding(&contract, &[Domain::new("x", -2, 2)], 10, &script).unwrap();
    assert_eq!(audit.cases, 10);
    assert_eq!(audit.obligations, [0, 2, 0, 0]);
}

#[test]
fn signed_boundaries_include_requires_and_substituted_body_overflow() {
    let pre = one_parameter(
        Expr::Ge(
            Box::new(Expr::Sub(Box::new(x()), Box::new(Expr::I64(1)))),
            Box::new(Expr::I64(i64::MIN)),
        ),
        Expr::Bool(true),
    );
    let audit = audit_encoding(
        &pre,
        &[Domain::new("x", i64::MIN, i64::MIN + 1)],
        2,
        &emit_smtlib(&pre).unwrap(),
    )
    .unwrap();
    assert_eq!(audit.obligations, [1, 1, 0, 0]);

    let body = Expr::Add(Box::new(x()), Box::new(Expr::I64(1)));
    let contract = one_parameter(
        Expr::Bool(true),
        Expr::And(
            Box::new(Expr::Eq(Box::new(body.clone()), Box::new(body))),
            Box::new(Expr::Bool(true)),
        ),
    );
    let audit = audit_encoding(
        &contract,
        &[Domain::new("x", i64::MAX - 1, i64::MAX)],
        2,
        &emit_smtlib(&contract).unwrap(),
    )
    .unwrap();
    assert_eq!(audit.obligations, [0, 2, 0, 1]);
}

#[test]
fn malformed_and_mismatched_formulas_fail_closed() {
    let contract = one_parameter(Expr::Bool(true), Expr::Bool(true));
    let script = emit_smtlib(&contract).unwrap();
    let domain = [Domain::new("x", 0, 1)];
    let truncated = script.split("; vc:ensures_overflow").next().unwrap();
    assert!(matches!(
        audit_encoding(&contract, &domain, 2, truncated),
        Err(AuditError::MalformedFormula(_))
    ));
    let changed = script.replacen("(assert (not true))", "(assert (not false))", 1);
    assert!(matches!(
        audit_encoding(&contract, &domain, 2, &changed),
        Err(AuditError::Mismatch {
            obligation: "requires_defined",
            ..
        })
    ));
    assert!(matches!(
        audit_encoding(&contract, &domain, 1, &script),
        Err(AuditError::InvalidInput(_))
    ));
    assert!(matches!(
        audit_encoding_with_source(&contract, &domain, 2, &script, &|values| {
            assert!(matches!(values.get("x"), Some(Value::I64(_))));
            Ok([false, true, true, false])
        }),
        Err(AuditError::SourceMismatch { .. })
    ));
}
