use std::time::Duration;

use cgx2_verify_core::{
    emit_smtlib, parse_solver_result, run_solver, Contract, Expr, Parameter, SolverExit,
    SolverResponse, Sort, UnknownReason,
};

fn increment_contract() -> Contract {
    Contract {
        parameters: vec![Parameter::new("x", Sort::I64)],
        requires: Expr::Bool(true),
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
fn emits_i64_bounds_overflow_guards_and_three_checks() {
    let text = emit_smtlib(&increment_contract()).unwrap();
    assert!(text.contains("(set-logic QF_UFLIA)"));
    assert!(text.contains("(>= v_78 (- 9223372036854775808))"));
    assert!(text.contains("(<= v_78 9223372036854775807)"));
    assert!(text.contains("(<= (+ v_78 1) 9223372036854775807)"));
    assert!(text.contains("(not (and"));
    assert_eq!(text.matches("(check-sat)").count(), 4);
}

#[test]
fn symbol_encoding_cannot_inject_smtlib() {
    let contract = Contract {
        parameters: vec![Parameter::new("x) (assert false)", Sort::Bool)],
        requires: Expr::Bool(true),
        ensures: Expr::Var("x) (assert false)".into()),
    };
    let text = emit_smtlib(&contract).unwrap();
    assert!(!text.contains("(assert false)"));
}

#[test]
fn uninterpreted_calls_keep_argument_sorts_and_disclose_i64_bounds() {
    let contract = Contract {
        parameters: vec![Parameter::new("flag", Sort::Bool)],
        requires: Expr::Bool(true),
        ensures: Expr::Eq(
            Box::new(Expr::Uninterpreted {
                name: "foreign".into(),
                sort: Sort::I64,
                args: vec![Expr::Var("flag".into())],
            }),
            Box::new(Expr::I64(0)),
        ),
    };
    let text = emit_smtlib(&contract).unwrap();
    assert!(text.contains("(Bool) Int)"), "{text}");
    assert!(!text.contains("forall"), "{text}");
    assert!(text.contains("; Uninterpreted functions"));
    assert!(text.contains("(assert (and (>= (u_"), "{text}");
}

#[test]
fn protocol_requires_valid_precondition_and_unsat_negation() {
    assert_eq!(
        parse_solver_result("unsat\nsat\nunsat\nunsat\n", "", SolverExit::Code(0)),
        SolverResponse::Unsat
    );
    assert_eq!(
        parse_solver_result("unsat\nsat\nsat\nunsat\n", "", SolverExit::Code(0)),
        SolverResponse::SatCandidate
    );
    assert_eq!(
        parse_solver_result("unsat\nunsat\nunsat\nunsat\n", "", SolverExit::Code(0)),
        SolverResponse::Unknown(UnknownReason::InconsistentPrecondition)
    );
    assert!(matches!(
        parse_solver_result("sat\nsat\nunsat\nunsat\n", "", SolverExit::Code(0)),
        SolverResponse::Unknown(_)
    ));
}

#[test]
fn protocol_rejects_unknown_timeout_missing_and_malformed() {
    assert_eq!(
        parse_solver_result("unsat\nsat\nunknown\nunsat\n", "", SolverExit::Code(0)),
        SolverResponse::Unknown(UnknownReason::SolverUnknown)
    );
    assert_eq!(
        parse_solver_result("", "", SolverExit::Timeout),
        SolverResponse::Unknown(UnknownReason::Timeout)
    );
    assert_eq!(
        parse_solver_result("", "", SolverExit::Unavailable),
        SolverResponse::Unknown(UnknownReason::SolverUnavailable)
    );
    assert_eq!(
        parse_solver_result("unsat\n", "", SolverExit::Code(0)),
        SolverResponse::Unknown(UnknownReason::MalformedResponse)
    );
    assert_eq!(
        parse_solver_result("unsat\nsat\nunsat\nunsat\n", "error", SolverExit::Code(0)),
        SolverResponse::Unknown(UnknownReason::MalformedResponse)
    );
}

#[cfg(unix)]
#[test]
fn fake_solver_command_covers_success_missing_and_timeout() {
    let script = emit_smtlib(&increment_contract()).unwrap();
    assert_eq!(
        run_solver(
            "/bin/sh",
            &["-c", "printf 'unsat\\nsat\\nunsat\\nunsat\\n'"],
            &script,
            Duration::from_secs(1)
        ),
        SolverResponse::Unsat
    );
    assert_eq!(
        run_solver(
            "/definitely/missing/cgx2-solver",
            &[],
            &script,
            Duration::from_secs(1)
        ),
        SolverResponse::Unknown(UnknownReason::SolverUnavailable)
    );
    assert_eq!(
        run_solver(
            "/bin/sh",
            &["-c", "sleep 1"],
            &script,
            Duration::from_millis(10)
        ),
        SolverResponse::Unknown(UnknownReason::Timeout)
    );
}
