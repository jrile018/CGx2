use cgx2_ownership::v1::*;
use cgx2_syntax::Span;

const SOURCE: &str = "α owner move borrow call return branch close loop";

fn span(text: &str) -> Span {
    let start = SOURCE.find(text).unwrap();
    Span {
        start,
        end: start + text.len(),
    }
}

fn local(name: &str) -> Local {
    Local {
        name: name.into(),
        storage: Storage::Owned,
        span: span("owner"),
    }
}

fn p(name: &str) -> Place {
    Place::local(name)
}
fn field(name: &str, field: &str) -> Place {
    p(name).project(Projection::Field(field.into()))
}
fn op(operation: Operation) -> SpannedOperation {
    at(operation, "borrow")
}
fn at(operation: Operation, text: &str) -> SpannedOperation {
    SpannedOperation {
        operation,
        span: span(text),
    }
}
fn block(operations: Vec<SpannedOperation>, terminator: Terminator) -> Block {
    Block {
        operations,
        terminator,
        span: span("return"),
    }
}
fn body(operations: Vec<SpannedOperation>) -> Function {
    Function {
        summary: summary("test", vec![], ResultMode::Unit, Retention::Local),
        locals: vec![local("a"), local("b"), local("c")],
        blocks: vec![block(operations, Terminator::Return(ReturnValue::Unit))],
        entry: 0,
        span: Span {
            start: 0,
            end: SOURCE.len(),
        },
    }
}
fn summary(
    name: &str,
    parameters: Vec<ParameterMode>,
    result: ResultMode,
    retention: Retention,
) -> FunctionSummary {
    FunctionSummary {
        name: name.into(),
        parameters,
        result,
        retention,
    }
}
fn create(name: &str) -> SpannedOperation {
    op(Operation::Create(name.into()))
}
fn borrow(place: Place, token: &str, kind: LoanKind) -> SpannedOperation {
    op(Operation::Borrow {
        place,
        token: token.into(),
        kind,
    })
}
fn call(name: &str, arguments: Vec<Place>, result: CallResult) -> SpannedOperation {
    at(
        Operation::Call {
            function: name.into(),
            arguments,
            result,
            retention_owner: None,
        },
        "call",
    )
}
fn run(f: &Function, summaries: &[FunctionSummary]) -> AnalysisReport {
    analyze_function(f, summaries, SOURCE, AnalysisLimits::default())
}
fn accepted(f: &Function, summaries: &[FunctionSummary]) {
    let report = run(f, summaries);
    assert!(report.is_accepted(), "{report:?}");
}
fn rejected(f: &Function, summaries: &[FunctionSummary], code: &str, at: Span) {
    let report = run(f, summaries);
    assert!(!report.is_accepted(), "unexpected acceptance");
    assert_eq!(report.diagnostics[0].code, code, "{report:?}");
    assert_eq!(report.diagnostics[0].span, at, "{report:?}");
}

#[test]
fn owned_call_consumes_argument_and_preserves_result() {
    let identity = summary(
        "identity",
        vec![ParameterMode::Owned],
        ResultMode::Owned {
            from_parameter: Some(0),
        },
        Retention::Local,
    );
    let mut f = body(vec![
        create("a"),
        call("identity", vec![p("a")], CallResult::Owned("b".into())),
        op(Operation::Read(p("b"))),
    ]);
    accepted(&f, std::slice::from_ref(&identity));
    f.blocks[0]
        .operations
        .push(at(Operation::Read(p("a")), "move"));
    rejected(&f, &[identity], "CGO101", span("move"));
}

#[test]
fn ordinary_borrowed_calls_release_temporary_loans() {
    for mode in [ParameterMode::Shared, ParameterMode::Exclusive] {
        let callee = summary("inspect", vec![mode], ResultMode::Unit, Retention::Local);
        let f = body(vec![
            create("a"),
            call("inspect", vec![p("a")], CallResult::Unit),
            op(Operation::Move {
                from: p("a"),
                to: "b".into(),
            }),
        ]);
        accepted(&f, &[callee]);
    }
}

#[test]
fn parameter_result_loans_are_checked_in_body_and_caller() {
    let contract = summary(
        "view",
        vec![ParameterMode::Shared],
        ResultMode::Loan {
            parameter: 0,
            projection: vec![Projection::Field("left".into())],
            kind: LoanKind::Shared,
        },
        Retention::Local,
    );
    let mut callee = body(vec![borrow(field("a", "left"), "result", LoanKind::Shared)]);
    callee.summary = contract.clone();
    callee.blocks[0].terminator = Terminator::Return(ReturnValue::Loan("result".into()));
    accepted(&callee, &[]);
    let mut caller = body(vec![
        create("a"),
        call("view", vec![p("a")], CallResult::Loan("view".into())),
        op(Operation::UseLoan {
            token: "view".into(),
            write: false,
        }),
        op(Operation::EndLoan("view".into())),
        op(Operation::Move {
            from: p("a"),
            to: "b".into(),
        }),
    ]);
    accepted(&caller, std::slice::from_ref(&contract));
    caller.blocks[0].operations.remove(3);
    rejected(&caller, &[contract], "CGO102", span("borrow"));
}

#[test]
fn exclusive_result_can_be_written_but_blocks_direct_access() {
    let contract = summary(
        "view",
        vec![ParameterMode::Exclusive],
        ResultMode::Loan {
            parameter: 0,
            projection: vec![],
            kind: LoanKind::Exclusive,
        },
        Retention::CallOnly,
    );
    let mut f = body(vec![
        create("a"),
        call("view", vec![p("a")], CallResult::Loan("v".into())),
        op(Operation::UseLoan {
            token: "v".into(),
            write: true,
        }),
        op(Operation::EndLoan("v".into())),
    ]);
    accepted(&f, std::slice::from_ref(&contract));
    f.blocks[0]
        .operations
        .insert(3, at(Operation::Read(p("a")), "owner"));
    rejected(&f, &[contract], "CGO102", span("owner"));
}

#[test]
fn local_borrow_cannot_be_returned_under_parameter_contract() {
    let mut f = body(vec![
        create("b"),
        borrow(p("b"), "result", LoanKind::Shared),
    ]);
    f.summary.parameters = vec![ParameterMode::Shared];
    f.summary.result = ResultMode::Loan {
        parameter: 0,
        projection: vec![],
        kind: LoanKind::Shared,
    };
    f.blocks[0].terminator = Terminator::Return(ReturnValue::Loan("result".into()));
    rejected(&f, &[], "CGO103", span("return"));
}

#[test]
fn returned_loan_must_match_projection_and_kind() {
    let mut f = body(vec![borrow(
        field("a", "right"),
        "result",
        LoanKind::Shared,
    )]);
    f.summary.parameters = vec![ParameterMode::Exclusive];
    f.summary.result = ResultMode::Loan {
        parameter: 0,
        projection: vec![Projection::Field("left".into())],
        kind: LoanKind::Shared,
    };
    f.blocks[0].terminator = Terminator::Return(ReturnValue::Loan("result".into()));
    rejected(&f, &[], "CGO103", span("return"));
    f.summary.result = ResultMode::Loan {
        parameter: 0,
        projection: vec![Projection::Field("right".into())],
        kind: LoanKind::Exclusive,
    };
    rejected(&f, &[], "CGO102", span("return"));
}

#[test]
fn owned_parameter_result_provenance_survives_moves() {
    let mut f = body(vec![op(Operation::Move {
        from: p("a"),
        to: "b".into(),
    })]);
    f.summary.parameters = vec![ParameterMode::Owned];
    f.summary.result = ResultMode::Owned {
        from_parameter: Some(0),
    };
    f.blocks[0].terminator = Terminator::Return(ReturnValue::Owned(p("b")));
    accepted(&f, &[]);
    f.summary.result = ResultMode::Owned {
        from_parameter: None,
    };
    rejected(&f, &[], "CGO103", span("return"));
}

#[test]
fn borrowed_parameters_cannot_be_consumed_or_strengthened() {
    let mut f = body(vec![op(Operation::Move {
        from: p("a"),
        to: "b".into(),
    })]);
    f.summary.parameters = vec![ParameterMode::Shared];
    rejected(&f, &[], "CGO102", span("borrow"));
    f.blocks[0].operations = vec![borrow(p("a"), "x", LoanKind::Exclusive)];
    rejected(&f, &[], "CGO102", span("borrow"));
}

#[test]
fn partial_move_keeps_sibling_available_and_allows_remainder_cleanup() {
    let f = body(vec![
        create("a"),
        op(Operation::Move {
            from: field("a", "left"),
            to: "b".into(),
        }),
        op(Operation::Read(field("a", "right"))),
        borrow(field("a", "right"), "x", LoanKind::Exclusive),
        op(Operation::UseLoan {
            token: "x".into(),
            write: true,
        }),
        op(Operation::EndLoan("x".into())),
        op(Operation::DropRemainder("a".into())),
        op(Operation::Drop(p("b"))),
    ]);
    accepted(&f, &[]);
}

#[test]
fn partial_move_forbids_parent_child_and_repeated_move() {
    for place in [
        p("a"),
        field("a", "left"),
        field("a", "left").project(Projection::Field("nested".into())),
    ] {
        let f = body(vec![
            create("a"),
            op(Operation::Move {
                from: field("a", "left"),
                to: "b".into(),
            }),
            at(Operation::Read(place), "move"),
        ]);
        rejected(&f, &[], "CGO101", span("move"));
    }
}

#[test]
fn known_siblings_are_disjoint_and_prefixes_conflict() {
    let pairs = [
        (field("a", "left"), field("a", "right")),
        (
            p("a").project(Projection::Tuple(0)),
            p("a").project(Projection::Tuple(1)),
        ),
        (
            p("a").project(Projection::Index(0)),
            p("a").project(Projection::Index(1)),
        ),
    ];
    for (a, b) in pairs {
        assert!(!a.overlaps(&b));
        let f = body(vec![
            create("a"),
            borrow(a, "x", LoanKind::Exclusive),
            borrow(b, "y", LoanKind::Exclusive),
            op(Operation::EndLoan("x".into())),
            op(Operation::EndLoan("y".into())),
        ]);
        accepted(&f, &[]);
    }
    let f = body(vec![
        create("a"),
        borrow(field("a", "left"), "x", LoanKind::Exclusive),
        at(Operation::Read(p("a")), "owner"),
    ]);
    rejected(&f, &[], "CGO102", span("owner"));
}

#[test]
fn shared_loans_coexist_and_writes_through_them_reject() {
    let mut f = body(vec![
        create("a"),
        borrow(p("a"), "x", LoanKind::Shared),
        borrow(p("a"), "y", LoanKind::Shared),
        op(Operation::EndLoan("x".into())),
        op(Operation::EndLoan("y".into())),
    ]);
    accepted(&f, &[]);
    f.blocks[0].operations.insert(
        3,
        at(
            Operation::UseLoan {
                token: "x".into(),
                write: true,
            },
            "owner",
        ),
    );
    rejected(&f, &[], "CGO102", span("owner"));
}

#[test]
fn dynamic_index_conflicts_and_cannot_transfer_unknown_element() {
    let dynamic = p("a").project(Projection::DynamicIndex);
    let f = body(vec![
        create("a"),
        borrow(dynamic.clone(), "x", LoanKind::Exclusive),
        at(
            Operation::Read(p("a").project(Projection::Index(7))),
            "owner",
        ),
    ]);
    rejected(&f, &[], "CGO102", span("owner"));
    let f = body(vec![
        create("a"),
        at(
            Operation::Move {
                from: dynamic,
                to: "b".into(),
            },
            "move",
        ),
    ]);
    rejected(&f, &[], "CGO105", span("move"));
    assert!(field("a", "left").overlaps(&p("a").project(Projection::Tuple(0))));
}

#[test]
fn calls_check_simultaneous_argument_aliases_before_consumption() {
    let callee = summary(
        "both",
        vec![ParameterMode::Owned, ParameterMode::Shared],
        ResultMode::Unit,
        Retention::Local,
    );
    let f = body(vec![
        create("a"),
        call("both", vec![p("a"), field("a", "left")], CallResult::Unit),
    ]);
    rejected(&f, &[callee], "CGO102", span("call"));
    let callee = summary(
        "both",
        vec![ParameterMode::Exclusive, ParameterMode::Shared],
        ResultMode::Unit,
        Retention::Local,
    );
    let f = body(vec![
        create("a"),
        call(
            "both",
            vec![field("a", "left"), field("a", "right")],
            CallResult::Unit,
        ),
    ]);
    accepted(&f, &[callee]);
}

fn diamond(
    left: Vec<SpannedOperation>,
    right: Vec<SpannedOperation>,
    after: Vec<SpannedOperation>,
) -> Function {
    let mut f = body(vec![]);
    f.blocks = vec![
        block(
            vec![create("a")],
            Terminator::Branch {
                then_block: 1,
                else_block: 2,
            },
        ),
        block(left, Terminator::Goto(3)),
        block(right, Terminator::Goto(3)),
        block(after, Terminator::Return(ReturnValue::Unit)),
    ];
    f
}

#[test]
fn cfg_join_preserves_possible_moves_and_possible_loans() {
    let f = diamond(
        vec![op(Operation::Move {
            from: p("a"),
            to: "b".into(),
        })],
        vec![],
        vec![at(Operation::Read(p("a")), "branch")],
    );
    rejected(&f, &[], "CGO101", span("branch"));
    let f = diamond(
        vec![borrow(p("a"), "x", LoanKind::Shared)],
        vec![],
        vec![at(Operation::Drop(p("a")), "branch")],
    );
    rejected(&f, &[], "CGO102", span("branch"));
}

#[test]
fn cfg_join_cannot_invent_a_definite_loan() {
    let f = diamond(
        vec![borrow(p("a"), "x", LoanKind::Shared)],
        vec![],
        vec![at(Operation::EndLoan("x".into()), "branch")],
    );
    rejected(&f, &[], "CGO102", span("branch"));
    let f = diamond(
        vec![borrow(p("a"), "x", LoanKind::Shared)],
        vec![borrow(p("a"), "x", LoanKind::Shared)],
        vec![op(Operation::EndLoan("x".into()))],
    );
    accepted(&f, &[]);
}

#[test]
fn cfg_join_preserves_partial_moves_on_both_arms() {
    let f = diamond(
        vec![op(Operation::Move {
            from: field("a", "left"),
            to: "b".into(),
        })],
        vec![op(Operation::Move {
            from: field("a", "right"),
            to: "c".into(),
        })],
        vec![at(Operation::Read(field("a", "left")), "branch")],
    );
    rejected(&f, &[], "CGO101", span("branch"));
}

fn looping(ops: Vec<SpannedOperation>) -> Function {
    let mut f = body(vec![]);
    f.blocks = vec![
        block(vec![create("a")], Terminator::Goto(1)),
        block(
            vec![],
            Terminator::Branch {
                then_block: 2,
                else_block: 3,
            },
        ),
        block(ops, Terminator::Goto(1)),
        block(vec![], Terminator::Return(ReturnValue::Unit)),
    ];
    f
}

#[test]
fn loop_fixed_point_handles_reinitialized_locals_and_balanced_loans() {
    let f = looping(vec![
        borrow(p("a"), "x", LoanKind::Shared),
        op(Operation::UseLoan {
            token: "x".into(),
            write: false,
        }),
        op(Operation::EndLoan("x".into())),
        create("b"),
        op(Operation::Drop(p("b"))),
    ]);
    let report = run(&f, &[]);
    assert!(report.is_accepted(), "{report:?}");
    assert!(
        report.stats.block_visits > f.blocks.len(),
        "expected fixed point revisit"
    );
    assert!(report.stats.steps < 100);
}

#[test]
fn loop_carried_invalid_loans_and_second_iteration_moves_reject() {
    let f = looping(vec![borrow(p("a"), "x", LoanKind::Shared)]);
    let report = run(&f, &[]);
    assert!(!report.is_accepted());
    assert_eq!(report.diagnostics[0].code, "CGO102");
    let f = looping(vec![at(Operation::Drop(p("a")), "loop")]);
    // The zero-trip exit is valid; the revisited body is not.
    rejected(&f, &[], "CGO101", span("loop"));
}

#[test]
fn step_and_state_budgets_stop_with_incomplete_rejection() {
    let f = looping(vec![op(Operation::Read(p("a")))]);
    for limits in [
        AnalysisLimits {
            max_steps: 1,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_state_entries: 1,
            ..AnalysisLimits::default()
        },
    ] {
        let report = analyze_function(&f, &[], SOURCE, limits);
        assert!(!report.complete && !report.is_accepted());
        assert_eq!(report.diagnostics[0].code, "CGO106");
        assert!(report.stats.steps <= limits.max_steps);
    }
}

#[test]
fn utf8_spans_are_preserved_and_malformed_spans_fail_closed() {
    let mut f = body(vec![at(Operation::Read(p("a")), "α")]);
    rejected(&f, &[], "CGO101", span("α"));
    f.blocks[0].operations[0].span = Span { start: 1, end: 2 };
    rejected(&f, &[], "CGO105", Span { start: 0, end: 0 });
    f.blocks[0].operations[0].span = Span {
        start: 0,
        end: SOURCE.len() + 1,
    };
    rejected(&f, &[], "CGO105", Span { start: 0, end: 0 });
}

fn retained(mode: ParameterMode) -> FunctionSummary {
    summary(
        "foreign",
        vec![mode],
        ResultMode::Unit,
        Retention::UntilRelease {
            parameters: vec![0],
        },
    )
}
fn retained_call() -> SpannedOperation {
    at(
        Operation::Call {
            function: "foreign".into(),
            arguments: vec![p("a")],
            result: CallResult::Unit,
            retention_owner: Some("handle".into()),
        },
        "call",
    )
}

#[test]
fn foreign_owner_retains_argument_until_explicit_release() {
    let callee = retained(ParameterMode::Shared);
    let f = body(vec![
        create("a"),
        retained_call(),
        op(Operation::Read(p("a"))),
        op(Operation::ReleaseForeign("handle".into())),
        op(Operation::Drop(p("a"))),
    ]);
    accepted(&f, std::slice::from_ref(&callee));
    let f = body(vec![
        create("a"),
        retained_call(),
        at(
            Operation::Move {
                from: p("a"),
                to: "b".into(),
            },
            "move",
        ),
    ]);
    rejected(&f, &[callee], "CGO102", span("move"));
    let f = body(vec![
        create("a"),
        retained_call(),
        at(Operation::Read(p("a")), "owner"),
    ]);
    rejected(
        &f,
        &[retained(ParameterMode::Exclusive)],
        "CGO102",
        span("owner"),
    );
}

#[test]
fn unknown_foreign_contract_and_missing_retention_owner_reject() {
    let unknown = summary(
        "foreign",
        vec![ParameterMode::Shared],
        ResultMode::Unit,
        Retention::Unknown,
    );
    let f = body(vec![
        create("a"),
        call("foreign", vec![p("a")], CallResult::Unit),
    ]);
    rejected(&f, &[unknown], "CGO104", span("call"));
    rejected(
        &f,
        &[retained(ParameterMode::Shared)],
        "CGO104",
        span("call"),
    );
    rejected(&f, &[], "CGO104", span("call"));
}

#[test]
fn retained_owners_cannot_escape_or_be_released_twice() {
    let mut f = body(vec![create("a"), retained_call()]);
    rejected(
        &f,
        &[retained(ParameterMode::Shared)],
        "CGO102",
        span("return"),
    );
    f.blocks[0].operations.extend([
        op(Operation::ReleaseForeign("handle".into())),
        at(Operation::ReleaseForeign("handle".into()), "close"),
    ]);
    rejected(
        &f,
        &[retained(ParameterMode::Shared)],
        "CGO104",
        span("close"),
    );
}

#[test]
fn local_regions_require_cleanup_and_cannot_escape() {
    let mut f = body(vec![op(Operation::OpenRegion("r".into())), create("a")]);
    f.locals[0].storage = Storage::Region("r".into());
    f.summary.result = ResultMode::Owned {
        from_parameter: None,
    };
    f.blocks[0].terminator = Terminator::Return(ReturnValue::Owned(p("a")));
    rejected(&f, &[], "CGO103", span("return"));
    f.summary.result = ResultMode::Unit;
    f.blocks[0].terminator = Terminator::Return(ReturnValue::Unit);
    f.blocks[0]
        .operations
        .push(at(Operation::CloseRegion("r".into()), "close"));
    rejected(&f, &[], "CGO103", span("close"));
    f.blocks[0]
        .operations
        .insert(2, op(Operation::Drop(p("a"))));
    accepted(&f, &[]);
}

#[test]
fn no_implicit_cross_region_move_or_call_result() {
    let mut f = body(vec![
        op(Operation::OpenRegion("r".into())),
        create("a"),
        at(
            Operation::Move {
                from: p("a"),
                to: "b".into(),
            },
            "move",
        ),
    ]);
    f.locals[0].storage = Storage::Region("r".into());
    rejected(&f, &[], "CGO103", span("move"));
    f.blocks[0].operations[2] = call("identity", vec![p("a")], CallResult::Owned("b".into()));
    let identity = summary(
        "identity",
        vec![ParameterMode::Owned],
        ResultMode::Owned {
            from_parameter: Some(0),
        },
        Retention::Local,
    );
    rejected(&f, &[identity], "CGO103", span("call"));
}

#[test]
fn caller_region_ownership_and_loans_can_be_returned_but_region_cannot_close() {
    let mut f = body(vec![op(Operation::Move {
        from: p("a"),
        to: "b".into(),
    })]);
    f.locals[0].storage = Storage::Region("caller".into());
    f.locals[1].storage = Storage::Region("caller".into());
    f.summary.parameters = vec![ParameterMode::Owned];
    f.summary.result = ResultMode::Owned {
        from_parameter: Some(0),
    };
    f.blocks[0].terminator = Terminator::Return(ReturnValue::Owned(p("b")));
    accepted(&f, &[]);
    f.blocks[0]
        .operations
        .insert(0, at(Operation::CloseRegion("caller".into()), "close"));
    rejected(&f, &[], "CGO103", span("close"));
}

#[test]
fn possible_regional_value_still_blocks_close_after_join() {
    let mut f = diamond(
        vec![op(Operation::Drop(p("a")))],
        vec![],
        vec![at(Operation::CloseRegion("r".into()), "close")],
    );
    f.blocks[0]
        .operations
        .insert(0, op(Operation::OpenRegion("r".into())));
    f.locals[0].storage = Storage::Region("r".into());
    rejected(&f, &[], "CGO103", span("close"));
}

#[test]
fn malformed_cfg_summary_and_unreachable_unsupported_are_rejected() {
    let mut f = body(vec![]);
    f.blocks[0].terminator = Terminator::Goto(99);
    rejected(&f, &[], "CGO105", span("return"));
    f.blocks[0].terminator = Terminator::Return(ReturnValue::Unit);
    let invalid = summary(
        "bad",
        vec![ParameterMode::Owned],
        ResultMode::Loan {
            parameter: 0,
            projection: vec![],
            kind: LoanKind::Shared,
        },
        Retention::Local,
    );
    rejected(&f, &[invalid], "CGO105", f.span);
    f.blocks.push(block(
        vec![at(
            Operation::Unsupported("pointer dereference".into()),
            "move",
        )],
        Terminator::Return(ReturnValue::Unit),
    ));
    rejected(&f, &[], "CGO105", span("move"));
}

#[test]
fn input_limits_are_explicit_and_rejection_is_deterministic() {
    let f = body(vec![
        create("a"),
        op(Operation::Read(
            field("a", "x").project(Projection::Tuple(0)),
        )),
    ]);
    for limits in [
        AnalysisLimits {
            max_blocks: 0,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_locals: 1,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_operations: 1,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_projection_depth: 1,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_name_bytes: 1,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_source_bytes: 1,
            ..AnalysisLimits::default()
        },
    ] {
        let a = analyze_function(&f, &[], SOURCE, limits);
        let b = analyze_function(&f, &[], SOURCE, limits);
        assert_eq!(a, b);
        assert!(!a.complete && !a.is_accepted());
        assert_eq!(a.diagnostics[0].code, "CGO106");
    }
}

#[test]
fn diagnostic_includes_valid_related_loan_span() {
    let f = body(vec![
        create("a"),
        borrow(p("a"), "x", LoanKind::Shared),
        at(Operation::Drop(p("a")), "move"),
    ]);
    let report = run(&f, &[]);
    assert_eq!(report.diagnostics[0].related, vec![span("borrow")]);
    assert_eq!(report.diagnostics[0].span, span("move"));
}
