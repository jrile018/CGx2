use std::collections::BTreeMap;

use cgx2_verify_core::{
    audit_encoding, emit_smtlib, eval, validate_solver_witness, verify_bounded, Contract, Domain,
    EvalError, Expr, Failure, Parameter, Sort, UnknownReason, Value, Verdict,
};

fn boxed(expr: Expr) -> Box<Expr> {
    Box::new(expr)
}
fn overflow() -> Expr {
    Expr::Add(boxed(Expr::I64(i64::MAX)), boxed(Expr::I64(1)))
}
fn choose(guard: Expr, yes: Expr, no: Expr) -> Expr {
    Expr::If(boxed(guard), boxed(yes), boxed(no))
}

#[test]
fn evaluation_checks_guard_and_only_selected_branch() {
    let values = BTreeMap::new();
    for guard in [false, true] {
        let body = if guard {
            choose(Expr::Bool(true), Expr::I64(7), overflow())
        } else {
            choose(Expr::Bool(false), overflow(), Expr::I64(7))
        };
        assert_eq!(eval(&body, &values), Ok(Value::I64(7)));
    }
    assert_eq!(
        eval(&choose(Expr::Bool(true), overflow(), Expr::I64(0)), &values),
        Err(EvalError::Overflow)
    );
    let guard = Expr::Gt(boxed(overflow()), boxed(Expr::I64(0)));
    assert_eq!(
        eval(&choose(guard, Expr::Bool(true), Expr::Bool(false)), &values),
        Err(EvalError::Overflow)
    );
    assert_eq!(
        eval(&choose(Expr::I64(0), Expr::I64(1), Expr::I64(2)), &values),
        Err(EvalError::SortMismatch)
    );
}

#[test]
fn sorts_and_uninterpreted_walkers_examine_both_branches() {
    let sorts = BTreeMap::new();
    assert_eq!(
        choose(Expr::Bool(true), Expr::Bool(false), Expr::Bool(true)).sort(&sorts),
        Ok(Sort::Bool)
    );
    assert!(choose(Expr::Bool(true), Expr::I64(0), Expr::Bool(false))
        .sort(&sorts)
        .is_err());
    assert!(choose(Expr::I64(1), Expr::I64(0), Expr::I64(0))
        .sort(&sorts)
        .is_err());
    let body = choose(
        Expr::Bool(true),
        Expr::Bool(true),
        Expr::Uninterpreted {
            name: "foreign".into(),
            sort: Sort::Bool,
            args: vec![],
        },
    );
    assert!(body.contains_uninterpreted());
    let contract = Contract {
        parameters: vec![],
        requires: Expr::Bool(true),
        ensures: body,
    };
    assert!(matches!(
        verify_bounded(&contract, &[], 1),
        Verdict::Unknown {
            reason: UnknownReason::Unsupported(_)
        }
    ));
}

#[test]
fn smt_definedness_and_witness_replay_follow_selected_branch() {
    let flag = Expr::Var("flag".into());
    let x = Expr::Var("x".into());
    let body = choose(
        flag,
        x.clone(),
        Expr::Add(boxed(x.clone()), boxed(Expr::I64(1))),
    );
    let contract = Contract {
        parameters: vec![
            Parameter::new("flag", Sort::Bool),
            Parameter::new("x", Sort::I64),
        ],
        requires: Expr::Bool(true),
        ensures: Expr::Eq(boxed(body.clone()), boxed(body)),
    };
    let script = emit_smtlib(&contract).unwrap();
    assert!(script.contains("(ite v_666c6167 v_78 (+ v_78 1))"));
    assert!(script.contains("(and true (ite v_666c6167 true (and true true"));
    let audit = audit_encoding(
        &contract,
        &[Domain::new("x", i64::MAX - 1, i64::MAX)],
        4,
        &script,
    )
    .unwrap();
    assert_eq!(audit.obligations, [0, 4, 0, 1]);
    let model = "sat\n((v_666c6167 false) (v_78 9223372036854775807))\n";
    assert_eq!(
        validate_solver_witness(&contract, model, Failure::Overflow)
            .unwrap()
            .values["flag"],
        Value::Bool(false)
    );
    assert!(validate_solver_witness(
        &contract,
        &model.replace("false", "true"),
        Failure::Overflow
    )
    .is_err());
    let domains = [Domain::new("x", i64::MAX, i64::MAX)];
    assert!(matches!(
        verify_bounded(&contract, &domains, 2),
        Verdict::Disproved { .. }
    ));
}
