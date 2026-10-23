use cgx2_verify_core::scalar_v2::*;

fn contract(ensures: Expr) -> Contract {
    Contract {
        parameters: vec![],
        requires: Expr::Bool(true),
        ensures,
    }
}
fn equality(value: Expr, expected: i64) -> Contract {
    contract(Expr::Eq(Box::new(value), Box::new(Expr::I64(expected))))
}
fn divisions(n: usize) -> Contract {
    let mut value = Expr::I64(-7);
    for _ in 0..n {
        value = Expr::Div(Box::new(value), Box::new(Expr::I64(1)));
    }
    equality(value, -7)
}
fn audit(c: &Contract) -> String {
    let text = emit_smtlib(c).unwrap();
    let checked = audit_encoding(c, &[], 1, &text).unwrap();
    assert_eq!(checked.obligations, [0, 1, 0, 0]);
    text
}
#[test]
fn nested_division_has_measured_bounded_growth_and_real_term_parity() {
    let mut previous = 0;
    for n in [1, 2, 3, 4, 8, 16, 32, 64, 120] {
        let c = divisions(n);
        let text = audit(&c);
        let witness = emit_witness_smtlib(&c, Failure::Overflow).unwrap();
        println!(
            "divisions={n} smt_bytes={} witness_bytes={}",
            text.len(),
            witness.len()
        );
        assert!(text.len() < 1800 * n + 2000);
        assert!(witness.len() < text.len());
        if previous != 0 {
            assert!(text.len() < previous * 3);
        }
        previous = text.len();
    }
}
#[test]
fn exact_node_depth_and_output_budgets_apply_to_main_and_witness() {
    let c = divisions(4); // requires + Eq + four (Div,literal) pairs + two literals
    let limits = SmtLimits {
        nodes: 12,
        depth: 6,
        ..SmtLimits::default()
    };
    let script = emit_smtlib_with_limits(&c, limits).unwrap();
    assert_eq!(script, emit_smtlib(&c).unwrap());
    for smaller in [
        SmtLimits {
            nodes: 11,
            ..limits
        },
        SmtLimits { depth: 5, ..limits },
    ] {
        assert!(matches!(
            emit_smtlib_with_limits(&c, smaller),
            Err(SmtError::LimitExceeded { .. })
        ));
        assert!(matches!(
            emit_witness_smtlib_with_limits(&c, Failure::Overflow, smaller),
            Err(SmtError::LimitExceeded { .. })
        ));
    }
    let exact = SmtLimits {
        output_bytes: script.len(),
        ..limits
    };
    assert_eq!(
        emit_smtlib_with_limits(&c, exact).unwrap().len(),
        script.len()
    );
    assert_eq!(
        emit_smtlib_with_limits(
            &c,
            SmtLimits {
                output_bytes: script.len() - 1,
                ..limits
            }
        ),
        Err(SmtError::LimitExceeded {
            resource: "output_bytes",
            limit: script.len() - 1
        })
    );
    for failure in [Failure::Overflow, Failure::EnsuresFalse] {
        let witness = emit_witness_smtlib(&c, failure).unwrap();
        let exact = SmtLimits {
            output_bytes: witness.len(),
            ..limits
        };
        assert_eq!(
            emit_witness_smtlib_with_limits(&c, failure, exact).unwrap(),
            witness
        );
        assert!(matches!(
            emit_witness_smtlib_with_limits(
                &c,
                failure,
                SmtLimits {
                    output_bytes: witness.len() - 1,
                    ..limits
                }
            ),
            Err(SmtError::LimitExceeded {
                resource: "output_bytes",
                ..
            })
        ));
    }
    assert_eq!(
        emit_smtlib_with_limits(
            &c,
            SmtLimits {
                depth: SMT_DEPTH_LIMIT + 1,
                ..limits
            }
        ),
        Err(SmtError::InvalidLimits)
    );
}
fn boolean_tree(depth: usize) -> Expr {
    if depth == 0 {
        Expr::Bool(true)
    } else {
        Expr::And(
            Box::new(boolean_tree(depth - 1)),
            Box::new(boolean_tree(depth - 1)),
        )
    }
}
#[test]
fn default_node_and_depth_ceilings_reject_before_recursive_work() {
    let near = contract(boolean_tree(11)); // 4095 + requires = 4096
    audit(&near);
    let over = contract(boolean_tree(12));
    assert_eq!(
        emit_smtlib(&over),
        Err(SmtError::LimitExceeded {
            resource: "nodes",
            limit: SMT_NODE_LIMIT
        })
    );
    assert!(matches!(
        audit_encoding(&over, &[], 1, ""),
        Err(AuditError::InvalidInput(_))
    ));
    let mut deep = Expr::Bool(true);
    for _ in 1..SMT_DEPTH_LIMIT {
        deep = Expr::Not(Box::new(deep));
    }
    let near = contract(deep);
    let script = emit_smtlib(&near).unwrap();
    assert!(audit_encoding(&near, &[], 1, &script).is_ok());
    let over = contract(Expr::Not(Box::new(near.ensures)));
    assert_eq!(
        emit_witness_smtlib(&over, Failure::Overflow),
        Err(SmtError::LimitExceeded {
            resource: "depth",
            limit: SMT_DEPTH_LIMIT
        })
    );
}
#[test]
fn output_and_identifier_caps_cover_large_names_before_expansion() {
    let name = "x".repeat(SMT_OUTPUT_LIMIT / 5);
    let c = Contract {
        parameters: vec![Parameter::new(&name, Sort::I64)],
        requires: Expr::Bool(true),
        ensures: Expr::Eq(Box::new(Expr::Var(name)), Box::new(Expr::I64(1))),
    };
    assert!(matches!(
        emit_smtlib(&c),
        Err(SmtError::LimitExceeded {
            resource: "output_bytes",
            ..
        })
    ));
    let c = Contract {
        parameters: vec![Parameter::new("x".repeat(SMT_OUTPUT_LIMIT), Sort::Bool)],
        ..contract(Expr::Bool(true))
    };
    assert!(matches!(
        emit_smtlib(&c),
        Err(SmtError::LimitExceeded {
            resource: "identifier_bytes",
            ..
        })
    ));
}
#[test]
fn fresh_symbols_preserve_shadowing_initializer_scope_and_non_capture() {
    let local = Expr::Let(
        "cgx2_t_0".into(),
        Box::new(Expr::I64(7)),
        Box::new(Expr::Let(
            "cgx2_t_0".into(),
            Box::new(Expr::Add(
                Box::new(Expr::Var("cgx2_t_0".into())),
                Box::new(Expr::I64(1)),
            )),
            Box::new(Expr::Var("cgx2_t_0".into())),
        )),
    );
    let c = Contract {
        parameters: vec![
            Parameter::new("cgx2_t_0", Sort::I64),
            Parameter::new("v_78", Sort::Bool),
        ],
        ..equality(local, 8)
    };
    let script = emit_smtlib(&c).unwrap();
    assert_eq!(
        audit_encoding(&c, &[Domain::new("cgx2_t_0", -1, 1)], 6, &script)
            .unwrap()
            .cases,
        6
    );
}
#[test]
fn inactive_undefined_chains_and_eager_boolean_and_local_failures_match() {
    let mut invalid = Expr::Mul(Box::new(Expr::I64(i64::MAX)), Box::new(Expr::I64(i64::MAX)));
    for _ in 0..32 {
        invalid = Expr::Mul(Box::new(invalid), Box::new(Expr::I64(i64::MAX)));
    }
    audit(&equality(
        Expr::If(
            Box::new(Expr::Bool(true)),
            Box::new(Expr::I64(7)),
            Box::new(invalid.clone()),
        ),
        7,
    ));
    for value in [
        invalid,
        Expr::Div(Box::new(Expr::I64(i64::MIN)), Box::new(Expr::I64(-1))),
        Expr::Div(Box::new(Expr::I64(1)), Box::new(Expr::I64(0))),
    ] {
        let c = equality(
            Expr::Let("unused".into(), Box::new(value), Box::new(Expr::I64(0))),
            0,
        );
        let c = contract(Expr::And(Box::new(Expr::Bool(false)), Box::new(c.ensures)));
        assert_eq!(
            audit_encoding(&c, &[], 1, &emit_smtlib(&c).unwrap())
                .unwrap()
                .obligations,
            [0, 1, 0, 1]
        );
    }
}
#[test]
fn corrupted_definitions_forward_references_sorts_and_obligations_are_rejected() {
    let c = divisions(4);
    let text = audit(&c);
    for bad in [
        text.replace("(div ", "(+ "),
        text.replace("cgx2_t_0 () Bool true", "cgx2_t_0 () Bool false"),
        text.replace("cgx2_t_0 () Bool true", "cgx2_t_0 () Bool cgx2_t_999999"),
        text.replace("cgx2_t_0 () Bool true", "cgx2_t_0 () Int true"),
        text.replace("; vc:ensures_false", "; vc:requires_defined"),
        format!("{text}(define-fun cgx2_t_0 () Bool true)\n"),
    ] {
        assert_ne!(bad, text);
        assert!(audit_encoding(&c, &[], 1, &bad).is_err());
    }
    assert!(audit_encoding(&c, &[], 1, &" ".repeat(SMT_OUTPUT_LIMIT + 1)).is_err());
}
#[test]
fn uninterpreted_range_assumptions_and_local_rejection_are_preserved() {
    let foreign = Expr::Uninterpreted {
        name: "f".into(),
        sort: Sort::I64,
        args: vec![Expr::I64(7)],
    };
    let c = equality(foreign.clone(), 7);
    let text = emit_smtlib(&c).unwrap();
    assert!(text.contains("(declare-fun u_"));
    assert!(text.contains("(assert (and (>= cgx2_t_"));
    assert!(matches!(
        audit_encoding(&c, &[], 1, &text),
        Err(AuditError::Unsupported(_))
    ));
    assert_eq!(
        emit_smtlib(&equality(
            Expr::Let("x".into(), Box::new(foreign), Box::new(Expr::I64(7))),
            7
        )),
        Err(SmtError::UnmodeledLocalBinding)
    );
}
