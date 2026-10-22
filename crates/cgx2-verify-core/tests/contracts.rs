use std::collections::BTreeMap;

use cgx2_verify_core::{
    eval, verify_bounded, Contract, Domain, EvalError, Expr, Failure, Parameter, Sort,
    UnknownReason, Value, Verdict,
};

#[test]
fn evaluates_boolean_and_checked_integer_expressions() {
    let expr = Expr::And(
        Box::new(Expr::Var("enabled".into())),
        Box::new(Expr::Eq(
            Box::new(Expr::Add(
                Box::new(Expr::Var("x".into())),
                Box::new(Expr::I64(2)),
            )),
            Box::new(Expr::I64(5)),
        )),
    );
    let contract = Contract {
        parameters: vec![
            Parameter::new("enabled", Sort::Bool),
            Parameter::new("x", Sort::I64),
        ],
        requires: Expr::Bool(true),
        ensures: expr.clone(),
    };
    assert_eq!(contract.validate(), Ok(()));
    let values = BTreeMap::from([
        ("enabled".into(), Value::Bool(true)),
        ("x".into(), Value::I64(3)),
    ]);
    assert_eq!(eval(&expr, &values), Ok(Value::Bool(true)));
}

#[test]
fn checked_addition_and_subtraction_reject_overflow() {
    let values = BTreeMap::new();
    assert_eq!(
        eval(
            &Expr::Add(Box::new(Expr::I64(i64::MAX)), Box::new(Expr::I64(1))),
            &values
        ),
        Err(EvalError::Overflow)
    );
    assert_eq!(
        eval(
            &Expr::Sub(Box::new(Expr::I64(i64::MIN)), Box::new(Expr::I64(1))),
            &values
        ),
        Err(EvalError::Overflow)
    );
}

#[test]
fn rejects_sort_mismatch_and_unknown_variable() {
    let contract = Contract {
        parameters: vec![Parameter::new("x", Sort::I64)],
        requires: Expr::Bool(true),
        ensures: Expr::And(Box::new(Expr::Var("x".into())), Box::new(Expr::Bool(true))),
    };
    assert!(contract.validate().is_err());
    let missing = Contract {
        ensures: Expr::Var("missing".into()),
        ..contract
    };
    assert!(missing.validate().is_err());
    assert_eq!(
        eval(&Expr::Var("missing".into()), &BTreeMap::new()),
        Err(EvalError::UnknownVariable("missing".into()))
    );
}

fn integer_contract(requires: Expr, ensures: Expr) -> Contract {
    Contract {
        parameters: vec![Parameter::new("x", Sort::I64)],
        requires,
        ensures,
    }
}

#[test]
fn proves_only_the_named_finite_range() {
    let contract = integer_contract(
        Expr::Bool(true),
        Expr::Le(Box::new(Expr::Var("x".into())), Box::new(Expr::I64(3))),
    );
    let verdict = verify_bounded(&contract, &[Domain::new("x", -2, 3)], 6);
    match verdict {
        Verdict::Proved { scope } => {
            assert_eq!(scope.domains, vec![Domain::new("x", -2, 3)]);
            assert_eq!(scope.cases, 6);
            assert!(scope.assumptions.iter().any(|s| s.contains("checked i64")));
        }
        other => panic!("expected bounded proof, got {other:?}"),
    }
}

#[test]
fn returns_smallest_validated_counterexample() {
    let contract = integer_contract(
        Expr::Bool(true),
        Expr::Gt(Box::new(Expr::Var("x".into())), Box::new(Expr::I64(0))),
    );
    match verify_bounded(&contract, &[Domain::new("x", -2, 2)], 5) {
        Verdict::Disproved { counterexample } => {
            assert_eq!(counterexample.values["x"], Value::I64(-2));
            assert_eq!(counterexample.failure, Failure::EnsuresFalse);
        }
        other => panic!("expected counterexample, got {other:?}"),
    }
}

#[test]
fn impossible_precondition_is_not_a_proof() {
    let contract = integer_contract(Expr::Bool(false), Expr::Bool(false));
    assert!(matches!(
        verify_bounded(&contract, &[Domain::new("x", 0, 1)], 2),
        Verdict::Unknown {
            reason: UnknownReason::InconsistentPrecondition
        }
    ));
}

#[test]
fn checked_overflow_is_a_failure_with_a_witness() {
    let contract = integer_contract(
        Expr::Bool(true),
        Expr::Eq(
            Box::new(Expr::Add(
                Box::new(Expr::Var("x".into())),
                Box::new(Expr::I64(1)),
            )),
            Box::new(Expr::Var("x".into())),
        ),
    );
    match verify_bounded(&contract, &[Domain::new("x", i64::MAX, i64::MAX)], 1) {
        Verdict::Disproved { counterexample } => {
            assert_eq!(counterexample.failure, Failure::Overflow)
        }
        other => panic!("expected overflow witness, got {other:?}"),
    }
}

#[test]
fn empty_domain_and_case_cap_are_unknown() {
    let contract = integer_contract(Expr::Bool(true), Expr::Bool(true));
    assert!(matches!(
        verify_bounded(&contract, &[Domain::new("x", 2, 1)], 10),
        Verdict::Unknown {
            reason: UnknownReason::EmptyDomain(_)
        }
    ));
    assert!(matches!(
        verify_bounded(&contract, &[Domain::new("x", 0, 10)], 5),
        Verdict::Unknown {
            reason: UnknownReason::CaseCap { max_cases: 5 }
        }
    ));
}

#[test]
fn boolean_parameters_are_exhaustively_enumerated() {
    let contract = Contract {
        parameters: vec![Parameter::new("b", Sort::Bool)],
        requires: Expr::Bool(true),
        ensures: Expr::Or(
            Box::new(Expr::Var("b".into())),
            Box::new(Expr::Not(Box::new(Expr::Var("b".into())))),
        ),
    };
    match verify_bounded(&contract, &[], 2) {
        Verdict::Proved { scope } => assert_eq!(scope.boolean_parameters, vec!["b"]),
        other => panic!("expected proof, got {other:?}"),
    }
}
