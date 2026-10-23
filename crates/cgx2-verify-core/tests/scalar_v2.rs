use cgx2_verify_core::scalar_v2::*;
use std::collections::BTreeMap;

fn binary(op: fn(Box<Expr>, Box<Expr>) -> Expr, a: i64, b: i64) -> Expr {
    op(Box::new(Expr::I64(a)), Box::new(Expr::I64(b)))
}

fn defined(body: Expr) -> Contract {
    Contract {
        parameters: vec![],
        requires: Expr::Bool(true),
        ensures: Expr::Eq(Box::new(body.clone()), Box::new(body)),
    }
}

#[test]
fn boundary_grid_matches_checked_rust_and_independent_smt_interpreter() {
    let boundary = [
        i64::MIN,
        i64::MIN + 1,
        -7,
        -3,
        -1,
        0,
        1,
        2,
        3,
        7,
        i64::MAX / 2,
        i64::MAX,
    ];
    for a in boundary {
        for b in boundary {
            for (op, expected) in [
                (Expr::Mul as fn(_, _) -> _, a.checked_mul(b)),
                (Expr::Div, a.checked_div(b)),
            ] {
                let expression = binary(op, a, b);
                assert_eq!(
                    eval(&expression, &BTreeMap::new()),
                    expected.map(Value::I64).ok_or(EvalError::Overflow)
                );
                let contract = defined(expression);
                let script = emit_smtlib(&contract).unwrap();
                let audit = audit_encoding(&contract, &[], 1, &script).unwrap();
                assert_eq!(
                    audit.obligations,
                    [0, 1, 0, usize::from(expected.is_none())],
                    "{a}, {b}"
                );
            }
        }
    }
}

#[test]
fn bindings_are_eager_lexical_and_conditionals_remain_lazy() {
    let body = Expr::Let(
        "x".into(),
        Box::new(Expr::I64(3)),
        Box::new(Expr::Let(
            "x".into(),
            Box::new(binary(Expr::Mul, 2, 3)),
            Box::new(Expr::Var("x".into())),
        )),
    );
    assert_eq!(eval(&body, &BTreeMap::new()), Ok(Value::I64(6)));
    let bad = Expr::Let(
        "unused".into(),
        Box::new(binary(Expr::Div, 1, 0)),
        Box::new(body.clone()),
    );
    assert_eq!(eval(&bad, &BTreeMap::new()), Err(EvalError::Overflow));
    let contract = defined(Expr::If(
        Box::new(Expr::Bool(true)),
        Box::new(body),
        Box::new(bad),
    ));
    assert_eq!(
        audit_encoding(&contract, &[], 1, &emit_smtlib(&contract).unwrap())
            .unwrap()
            .obligations,
        [0, 1, 0, 0]
    );
}

#[test]
fn corruption_of_new_operators_or_binding_is_detected() {
    for expression in [
        binary(Expr::Mul, 2, 3),
        binary(Expr::Div, -7, 3),
        Expr::Let(
            "x".into(),
            Box::new(Expr::I64(6)),
            Box::new(Expr::Var("x".into())),
        ),
    ] {
        let contract = Contract {
            parameters: vec![],
            requires: Expr::Bool(true),
            ensures: Expr::Eq(
                Box::new(expression.clone()),
                Box::new(Expr::I64(
                    match eval(&expression, &BTreeMap::new()).unwrap() {
                        Value::I64(n) => n,
                        _ => unreachable!(),
                    },
                )),
            ),
        };
        let script = emit_smtlib(&contract).unwrap();
        let corrupted = script
            .replace("(* 2 3)", "(+ 2 3)")
            .replace("(div ", "(+ ")
            .replace("((v_78 6))", "((v_78 5))");
        assert_ne!(script, corrupted);
        assert!(audit_encoding(&contract, &[], 1, &corrupted).is_err());
    }
}

fn accumulator(start: i64, end: i64) -> LiteralAccumulator {
    LiteralAccumulator {
        name: "total".into(),
        initial: Expr::Var("x".into()),
        index: "i".into(),
        start,
        end,
        increment: Expr::Var("i".into()),
        result: Expr::Add(
            Box::new(Expr::Var("total".into())),
            Box::new(Expr::Var("i".into())),
        ),
    }
}

#[test]
fn expansion_has_explicit_preallocation_bound_and_independent_loop_parity() {
    assert_eq!(
        accumulator(0, 3).expand(0),
        Err(ExpansionError::InvalidBound)
    );
    assert!(matches!(
        accumulator(i64::MIN, i64::MAX).expand(64),
        Err(ExpansionError::LimitExceeded { .. })
    ));
    assert!(matches!(
        accumulator(0, 3).expand(2),
        Err(ExpansionError::LimitExceeded { .. })
    ));
    for (start, end) in [(0, 3), (-3, 2), (3, 0), (0, 0), (0, 64)] {
        let body = accumulator(start, end).expand(64).unwrap();
        let mut values =
            BTreeMap::from([("x".into(), Value::I64(10)), ("i".into(), Value::I64(7))]);
        let mut total = 10_i64;
        for index in start..end {
            total = total.checked_add(index).unwrap();
        }
        assert_eq!(eval(&body, &values), Ok(Value::I64(total + 7)));
        let contract = Contract {
            parameters: vec![
                Parameter::new("x", Sort::I64),
                Parameter::new("i", Sort::I64),
            ],
            requires: Expr::Bool(true),
            ensures: Expr::Eq(Box::new(body), Box::new(Expr::I64(total + 7))),
        };
        let script = emit_smtlib(&contract).unwrap();
        let oracle = |_: &BTreeMap<String, Value>| Ok([false, true, false, false]);
        assert!(audit_encoding_with_source(
            &contract,
            &[Domain::new("x", 10, 10), Domain::new("i", 7, 7)],
            1,
            &script,
            &oracle
        )
        .is_ok());
        values.insert("x".into(), Value::I64(i64::MAX));
        if start == 0 && end == 3 {
            assert_eq!(eval(&contract.ensures, &values), Err(EvalError::Overflow));
        }
    }
}

#[test]
fn new_arithmetic_witnesses_are_replayed() {
    let contract = Contract {
        parameters: vec![Parameter::new("x", Sort::I64)],
        requires: Expr::Bool(true),
        ensures: Expr::Eq(
            Box::new(Expr::Div(
                Box::new(Expr::Var("x".into())),
                Box::new(Expr::I64(0)),
            )),
            Box::new(Expr::I64(0)),
        ),
    };
    assert!(validate_solver_witness(&contract, "sat\n((v_78 1))", Failure::Overflow).is_ok());
    assert!(validate_solver_witness(&contract, "sat\n((v_78 1))", Failure::EnsuresFalse).is_err());
    assert!(
        validate_solver_witness(&contract, "sat\n((v_78 garbage))", Failure::Overflow).is_err()
    );
}
