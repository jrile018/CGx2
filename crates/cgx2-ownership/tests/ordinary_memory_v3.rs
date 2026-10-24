use cgx2_ownership::{v2, v3};
use cgx2_syntax::Span;
use std::collections::BTreeSet;
fn span() -> Span {
    Span { start: 0, end: 1 }
}
fn ordinary(operation: v2::Operation) -> v3::SpannedOperation {
    v3::SpannedOperation {
        op: v3::Operation::Ordinary(operation),
        span: span(),
    }
}
fn example() -> v3::Function {
    let locals = [
        ("input", v3::Storage::Owned),
        ("carrier", v3::Storage::Owned),
        ("success", v3::Storage::Region("r".into())),
        ("failure", v3::Storage::Owned),
    ]
    .into_iter()
    .map(|(name, storage)| v3::Local {
        name: name.into(),
        storage,
        span: span(),
    })
    .collect();
    v3::Function {
        summary: v3::FunctionSummary {
            ordinary: v2::FunctionSummary {
                name: "transfer".into(),
                parameters: vec![],
                result: v3::ResultMode::Unit,
                retention: v3::Retention::Local,
            },
            region_parameters: vec![],
        },
        locals,
        transfer_carriers: BTreeSet::from(["carrier".into()]),
        entry: 0,
        span: span(),
        blocks: vec![
            v3::Block {
                span: span(),
                operations: vec![
                    ordinary(v2::Operation::OpenRegion("r".into())),
                    ordinary(v2::Operation::Create("input".into())),
                    v3::SpannedOperation {
                        op: v3::Operation::PrepareTransfer {
                            from: v3::Place::local("input"),
                            carrier: "carrier".into(),
                        },
                        span: span(),
                    },
                ],
                terminator: v3::Terminator::MatchTransfer {
                    carrier: "carrier".into(),
                    success: "success".into(),
                    failure: "failure".into(),
                    ok: 1,
                    err: 2,
                },
            },
            v3::Block {
                span: span(),
                operations: vec![
                    ordinary(v2::Operation::Read(v3::Place::local("success"))),
                    ordinary(v2::Operation::Drop(v3::Place::local("success"))),
                    ordinary(v2::Operation::CloseRegion("r".into())),
                ],
                terminator: v3::Terminator::Ordinary(v2::Terminator::Return(v3::ReturnValue::Unit)),
            },
            v3::Block {
                span: span(),
                operations: vec![
                    ordinary(v2::Operation::Read(v3::Place::local("failure"))),
                    ordinary(v2::Operation::Drop(v3::Place::local("failure"))),
                    ordinary(v2::Operation::CloseRegion("r".into())),
                ],
                terminator: v3::Terminator::Ordinary(v2::Terminator::Return(v3::ReturnValue::Unit)),
            },
        ],
    }
}
#[test]
fn both_outcomes_have_exactly_their_own_live_payload() {
    let report = v3::analyze_function(&example(), &[], "x", v3::AnalysisLimits::default());
    assert!(report.is_accepted(), "{report:?}");
}
#[test]
fn failed_owner_cannot_be_moved_twice_or_accessed_on_success() {
    for block in [1, 2] {
        let mut f = example();
        f.blocks[block]
            .operations
            .push(ordinary(v2::Operation::Read(v3::Place::local("failure"))));
        let report = v3::analyze_function(&f, &[], "x", v3::AnalysisLimits::default());
        assert!(!report.is_accepted(), "{report:?}");
        assert_eq!(report.diagnostics[0].code, "CGO101");
    }
}
#[test]
fn promotion_cannot_consume_an_owner_with_a_surviving_loan() {
    let mut f = example();
    f.blocks[0].operations.insert(
        2,
        ordinary(v2::Operation::Borrow {
            place: v3::Place::local("input"),
            token: "loan".into(),
            kind: v3::LoanKind::Shared,
        }),
    );
    let report = v3::analyze_function(&f, &[], "x", v3::AnalysisLimits::default());
    assert!(!report.is_accepted(), "{report:?}");
    assert_eq!(report.diagnostics[0].code, "CGO102");
}
