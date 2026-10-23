use cgx2_verify_core::{
    emit_smtlib, emit_witness_smtlib, parse_solver_result, validate_solver_witness, Contract, Expr,
    Failure, Parameter, SolverExit, SolverResponse, Sort, UnknownReason, Value,
};

fn contract() -> Contract {
    Contract {
        parameters: vec![
            Parameter::new("x", Sort::I64),
            Parameter::new("ok", Sort::Bool),
        ],
        requires: Expr::Var("ok".into()),
        ensures: Expr::Gt(
            Box::new(Expr::Add(
                Box::new(Expr::Var("x".into())),
                Box::new(Expr::I64(1)),
            )),
            Box::new(Expr::Var("x".into())),
        ),
    }
}

#[test]
fn verification_conditions_have_fixed_order_and_distinct_safety_query() {
    let script = emit_smtlib(&contract()).unwrap();
    assert_eq!(script.matches("(check-sat)").count(), 4);
    for label in [
        "requires_defined",
        "requires_satisfiable",
        "ensures_false",
        "ensures_overflow",
    ] {
        assert!(script.contains(label), "missing {label}");
    }
    assert!(script.find("requires_defined") < script.find("requires_satisfiable"));
    assert!(script.find("requires_satisfiable") < script.find("ensures_false"));
    assert!(script.find("ensures_false") < script.find("ensures_overflow"));
    let witness = emit_witness_smtlib(&contract(), Failure::Overflow).unwrap();
    assert!(witness.contains("(get-value (v_78 v_6f6b))"));
    assert_eq!(witness.matches("(check-sat)").count(), 1);
    for script in [&script, &witness] {
        let mut depth = 0;
        for line in script.lines().filter(|line| !line.starts_with(';')) {
            for byte in line.bytes() {
                match byte {
                    b'(' => depth += 1,
                    b')' => {
                        assert!(depth > 0, "unbalanced close: {line}");
                        depth -= 1;
                    }
                    _ => (),
                }
            }
        }
        assert_eq!(depth, 0, "unbalanced SMT script");
    }
}

#[test]
fn statuses_reject_contradictions_and_extra_output() {
    assert_eq!(
        parse_solver_result("unsat\nsat\nunsat\nunsat\n", "", SolverExit::Code(0)),
        SolverResponse::Unsat,
    );
    assert_eq!(
        parse_solver_result("sat\nsat\nunsat\nunsat\n", "", SolverExit::Code(0)),
        SolverResponse::Unknown(UnknownReason::Evaluation(
            "requires may overflow or produce an out-of-range value".into()
        )),
    );
    assert_eq!(
        parse_solver_result("unsat\nunsat\nunsat\nunsat\n", "", SolverExit::Code(0)),
        SolverResponse::Unknown(UnknownReason::InconsistentPrecondition),
    );
    for output in [
        "unsat\nsat\nsat\nunsat\nextra\n",
        "unsat\nsat\nunsat\n",
        "unsat\nsat\nunknown\nunsat\n",
        "unsat\n\nsat\nunsat\nunsat\n",
    ] {
        assert!(matches!(
            parse_solver_result(output, "", SolverExit::Code(0)),
            SolverResponse::Unknown(_)
        ));
    }
    assert_eq!(
        parse_solver_result(
            &"x".repeat(cgx2_verify_core::SOLVER_OUTPUT_LIMIT + 1),
            "",
            SolverExit::Code(0)
        ),
        SolverResponse::Unknown(UnknownReason::MalformedResponse),
    );
}

#[test]
fn model_is_only_a_counterexample_after_checked_reevaluation() {
    let witness = validate_solver_witness(
        &contract(),
        "sat\n((v_78 9223372036854775807) (v_6f6b true))\n",
        Failure::Overflow,
    )
    .unwrap();
    assert_eq!(witness.values["x"], Value::I64(i64::MAX));
    assert_eq!(witness.failure, Failure::Overflow);
    let minimum = Contract {
        parameters: vec![Parameter::new("x", Sort::I64)],
        requires: Expr::Bool(true),
        ensures: Expr::Ge(Box::new(Expr::Var("x".into())), Box::new(Expr::I64(0))),
    };
    let negative = validate_solver_witness(
        &minimum,
        "sat\n((v_78 (- 9223372036854775808)))\n",
        Failure::EnsuresFalse,
    )
    .unwrap();
    assert_eq!(negative.values["x"], Value::I64(i64::MIN));

    for model in [
        "sat\n((v_78 0) (v_6f6b true))\n",
        "sat\n((v_78 9223372036854775807) (v_6f6b false))\n",
        "sat\n((v_78 9223372036854775808) (v_6f6b true))\n",
        "sat\n((v_78 9223372036854775807))\n",
        "sat\n((v_78 9223372036854775807) (v_78 0) (v_6f6b true))\n",
        "sat\n((v_78 9223372036854775807) (v_6f6b true))\nextra\n",
        "unsat\n((v_78 9223372036854775807) (v_6f6b true))\n",
    ] {
        assert!(
            validate_solver_witness(&contract(), model, Failure::Overflow).is_err(),
            "{model}"
        );
    }
    assert!(validate_solver_witness(
        &contract(),
        &format!("sat{}", " ".repeat(16_385)),
        Failure::Overflow,
    )
    .is_err());
}
