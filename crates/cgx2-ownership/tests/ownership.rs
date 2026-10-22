use cgx2_ownership::{check_ops, Op, OwnershipDiagnostic};
use cgx2_syntax::Span;

fn s(n: usize) -> Span {
    Span {
        start: n,
        end: n + 1,
    }
}

fn assert_error(ops: &[Op], code: &'static str, span: Span) {
    let errors = check_ops(ops).expect_err("expected ownership error");
    assert!(
        errors
            .iter()
            .any(|error| error.code == code && error.span == span),
        "missing {code} at {span:?}: {errors:?}"
    );
}

#[test]
fn create_and_use_owned() {
    assert_eq!(
        check_ops(&[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0)
            },
            Op::Use {
                value: "a".into(),
                span: s(1)
            },
        ]),
        Ok(())
    );
}

#[test]
fn move_consumes_source_but_keeps_destination_live() {
    let ops = [
        Op::CreateOwned {
            value: "a".into(),
            span: s(0),
        },
        Op::Move {
            from: "a".into(),
            to: "b".into(),
            span: s(1),
        },
        Op::Use {
            value: "b".into(),
            span: s(2),
        },
    ];
    assert_eq!(check_ops(&ops), Ok(()));
    assert_error(
        &[
            ops[0].clone(),
            ops[1].clone(),
            Op::Use {
                value: "a".into(),
                span: s(3),
            },
        ],
        "CGO001",
        s(3),
    );
}

#[test]
fn double_move_is_rejected() {
    assert_error(
        &[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0),
            },
            Op::Move {
                from: "a".into(),
                to: "b".into(),
                span: s(1),
            },
            Op::Move {
                from: "a".into(),
                to: "c".into(),
                span: s(2),
            },
        ],
        "CGO001",
        s(2),
    );
}

#[test]
fn drop_consumes_value() {
    assert_error(
        &[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0),
            },
            Op::DropValue {
                value: "a".into(),
                span: s(1),
            },
            Op::Use {
                value: "a".into(),
                span: s(2),
            },
        ],
        "CGO001",
        s(2),
    );
}

#[test]
fn duplicate_name_and_unknown_value_have_op_spans() {
    assert_error(
        &[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0),
            },
            Op::CreateOwned {
                value: "a".into(),
                span: s(1),
            },
        ],
        "CGO001",
        s(1),
    );
    assert_error(
        &[Op::Use {
            value: "missing".into(),
            span: s(4),
        }],
        "CGO001",
        s(4),
    );
}

#[test]
fn diagnostics_are_deterministic() {
    let ops = [Op::Use {
        value: "missing".into(),
        span: s(4),
    }];
    let first: Vec<OwnershipDiagnostic> = check_ops(&ops).unwrap_err();
    assert_eq!(check_ops(&ops), Err(first));
}

#[test]
fn shared_borrows_coexist_and_end_independently() {
    assert_eq!(
        check_ops(&[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0)
            },
            Op::BorrowShared {
                value: "a".into(),
                token: "x".into(),
                span: s(1)
            },
            Op::BorrowShared {
                value: "a".into(),
                token: "y".into(),
                span: s(2)
            },
            Op::EndBorrow {
                token: "x".into(),
                span: s(3)
            },
            Op::EndBorrow {
                token: "y".into(),
                span: s(4)
            },
            Op::DropValue {
                value: "a".into(),
                span: s(5)
            },
        ]),
        Ok(())
    );
}

#[test]
fn mutable_borrow_is_exclusive() {
    assert_error(
        &[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0),
            },
            Op::BorrowShared {
                value: "a".into(),
                token: "x".into(),
                span: s(1),
            },
            Op::BorrowMut {
                value: "a".into(),
                token: "y".into(),
                span: s(2),
            },
        ],
        "CGO002",
        s(2),
    );
    assert_error(
        &[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0),
            },
            Op::BorrowMut {
                value: "a".into(),
                token: "x".into(),
                span: s(1),
            },
            Op::BorrowShared {
                value: "a".into(),
                token: "y".into(),
                span: s(2),
            },
        ],
        "CGO002",
        s(2),
    );
}

#[test]
fn move_drop_and_promote_while_borrowed_fail() {
    let prefix = [
        Op::CreateOwned {
            value: "a".into(),
            span: s(0),
        },
        Op::BorrowShared {
            value: "a".into(),
            token: "x".into(),
            span: s(1),
        },
    ];
    assert_error(
        &[
            prefix[0].clone(),
            prefix[1].clone(),
            Op::Move {
                from: "a".into(),
                to: "b".into(),
                span: s(2),
            },
        ],
        "CGO002",
        s(2),
    );
    assert_error(
        &[
            prefix[0].clone(),
            prefix[1].clone(),
            Op::DropValue {
                value: "a".into(),
                span: s(3),
            },
        ],
        "CGO002",
        s(3),
    );
    assert_error(
        &[
            Op::OpenRegion {
                region: "r".into(),
                span: s(4),
            },
            prefix[0].clone(),
            prefix[1].clone(),
            Op::Promote {
                from: "a".into(),
                to: "h".into(),
                region: "r".into(),
                span: s(5),
            },
        ],
        "CGO002",
        s(5),
    );
}

#[test]
fn unknown_and_duplicate_borrow_tokens_fail() {
    assert_error(
        &[Op::EndBorrow {
            token: "x".into(),
            span: s(1),
        }],
        "CGO002",
        s(1),
    );
    assert_error(
        &[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0),
            },
            Op::BorrowShared {
                value: "a".into(),
                token: "x".into(),
                span: s(1),
            },
            Op::BorrowShared {
                value: "a".into(),
                token: "x".into(),
                span: s(2),
            },
        ],
        "CGO002",
        s(2),
    );
}

#[test]
fn promote_requires_open_region_and_consumes_owned_source() {
    let good = [
        Op::OpenRegion {
            region: "r".into(),
            span: s(0),
        },
        Op::CreateOwned {
            value: "a".into(),
            span: s(1),
        },
        Op::Promote {
            from: "a".into(),
            to: "h".into(),
            region: "r".into(),
            span: s(2),
        },
        Op::Use {
            value: "h".into(),
            span: s(3),
        },
        Op::DropValue {
            value: "h".into(),
            span: s(4),
        },
        Op::CloseRegion {
            region: "r".into(),
            span: s(5),
        },
    ];
    assert_eq!(check_ops(&good), Ok(()));
    assert_error(
        &[
            good[0].clone(),
            good[1].clone(),
            good[2].clone(),
            Op::Use {
                value: "a".into(),
                span: s(6),
            },
        ],
        "CGO001",
        s(6),
    );
    assert_error(
        &[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0),
            },
            Op::Promote {
                from: "a".into(),
                to: "h".into(),
                region: "missing".into(),
                span: s(2),
            },
        ],
        "CGO003",
        s(2),
    );
    assert_error(
        &[
            Op::OpenRegion {
                region: "r".into(),
                span: s(0),
            },
            Op::CloseRegion {
                region: "r".into(),
                span: s(1),
            },
            Op::CreateOwned {
                value: "a".into(),
                span: s(2),
            },
            Op::Promote {
                from: "a".into(),
                to: "h".into(),
                region: "r".into(),
                span: s(3),
            },
        ],
        "CGO003",
        s(3),
    );
}

#[test]
fn copy_from_managed_keeps_source_live_and_creates_unique_owned_value() {
    let ops = [
        Op::OpenRegion {
            region: "r".into(),
            span: s(0),
        },
        Op::CreateManaged {
            value: "m".into(),
            region: "r".into(),
            span: s(1),
        },
        Op::CopyToOwned {
            from: "m".into(),
            to: "o".into(),
            span: s(2),
        },
        Op::Use {
            value: "m".into(),
            span: s(3),
        },
        Op::DropValue {
            value: "m".into(),
            span: s(4),
        },
        Op::CloseRegion {
            region: "r".into(),
            span: s(5),
        },
        Op::Use {
            value: "o".into(),
            span: s(6),
        },
    ];
    assert_eq!(check_ops(&ops), Ok(()));
    assert_error(
        &[
            ops[0].clone(),
            ops[1].clone(),
            Op::CopyToOwned {
                from: "m".into(),
                to: "m".into(),
                span: s(7),
            },
        ],
        "CGO001",
        s(7),
    );
    assert_error(
        &[
            Op::CreateOwned {
                value: "o".into(),
                span: s(0),
            },
            Op::CopyToOwned {
                from: "o".into(),
                to: "x".into(),
                span: s(8),
            },
        ],
        "CGO003",
        s(8),
    );
    assert_error(
        &[
            ops[0].clone(),
            ops[1].clone(),
            Op::BorrowMut {
                value: "m".into(),
                token: "t".into(),
                span: s(8),
            },
            Op::CopyToOwned {
                from: "m".into(),
                to: "o".into(),
                span: s(9),
            },
        ],
        "CGO002",
        s(9),
    );
}

#[test]
fn close_requires_no_live_handle_and_regions_are_isolated() {
    assert_error(
        &[
            Op::OpenRegion {
                region: "r".into(),
                span: s(0),
            },
            Op::CreateManaged {
                value: "h".into(),
                region: "r".into(),
                span: s(1),
            },
            Op::CloseRegion {
                region: "r".into(),
                span: s(2),
            },
        ],
        "CGO003",
        s(2),
    );
    assert_eq!(
        check_ops(&[
            Op::OpenRegion {
                region: "r1".into(),
                span: s(0)
            },
            Op::OpenRegion {
                region: "r2".into(),
                span: s(1)
            },
            Op::CreateManaged {
                value: "h".into(),
                region: "r2".into(),
                span: s(2)
            },
            Op::CloseRegion {
                region: "r1".into(),
                span: s(3)
            },
            Op::Use {
                value: "h".into(),
                span: s(4)
            },
            Op::DropValue {
                value: "h".into(),
                span: s(5)
            },
            Op::CloseRegion {
                region: "r2".into(),
                span: s(6)
            },
        ]),
        Ok(())
    );
}

#[test]
fn managed_handles_cannot_be_moved_or_promoted_as_owned() {
    let prefix = [
        Op::OpenRegion {
            region: "r".into(),
            span: s(0),
        },
        Op::CreateManaged {
            value: "h".into(),
            region: "r".into(),
            span: s(1),
        },
    ];
    assert_error(
        &[
            prefix[0].clone(),
            prefix[1].clone(),
            Op::Move {
                from: "h".into(),
                to: "other".into(),
                span: s(2),
            },
        ],
        "CGO001",
        s(2),
    );
    assert_error(
        &[
            prefix[0].clone(),
            prefix[1].clone(),
            Op::Promote {
                from: "h".into(),
                to: "other".into(),
                region: "r".into(),
                span: s(3),
            },
        ],
        "CGO003",
        s(3),
    );
}

#[test]
fn nested_no_gc_forbids_collection_until_all_guards_exit() {
    let prefix = [
        Op::OpenRegion {
            region: "r".into(),
            span: s(0),
        },
        Op::EnterNoGc { span: s(1) },
        Op::EnterNoGc { span: s(2) },
    ];
    assert_error(
        &[
            prefix[0].clone(),
            prefix[1].clone(),
            prefix[2].clone(),
            Op::Collect {
                region: "r".into(),
                span: s(3),
            },
        ],
        "CGO004",
        s(3),
    );
    assert_error(
        &[
            prefix[0].clone(),
            prefix[1].clone(),
            prefix[2].clone(),
            Op::ExitNoGc { span: s(4) },
            Op::Collect {
                region: "r".into(),
                span: s(5),
            },
        ],
        "CGO004",
        s(5),
    );
    assert_eq!(
        check_ops(&[
            prefix[0].clone(),
            prefix[1].clone(),
            prefix[2].clone(),
            Op::ExitNoGc { span: s(4) },
            Op::ExitNoGc { span: s(5) },
            Op::Collect {
                region: "r".into(),
                span: s(6)
            },
        ]),
        Ok(())
    );
}

#[test]
fn unmatched_no_gc_exit_and_open_guard_are_diagnosed() {
    assert_error(&[Op::ExitNoGc { span: s(1) }], "CGO005", s(1));
    assert_error(&[Op::EnterNoGc { span: s(2) }], "CGO005", s(2));
}

#[test]
fn branch_local_move_invalidates_source_at_join() {
    assert_error(
        &[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0),
            },
            Op::If {
                then_ops: vec![Op::Move {
                    from: "a".into(),
                    to: "b".into(),
                    span: s(2),
                }],
                else_ops: vec![],
                span: s(1),
            },
            Op::Use {
                value: "a".into(),
                span: s(3),
            },
        ],
        "CGO001",
        s(3),
    );
}

#[test]
fn borrow_must_end_in_its_branch() {
    assert_error(
        &[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0),
            },
            Op::If {
                then_ops: vec![Op::BorrowShared {
                    value: "a".into(),
                    token: "t".into(),
                    span: s(2),
                }],
                else_ops: vec![],
                span: s(1),
            },
        ],
        "CGO002",
        s(2),
    );
}

#[test]
fn balanced_branches_join_and_allow_later_use() {
    assert_eq!(
        check_ops(&[
            Op::CreateOwned {
                value: "a".into(),
                span: s(0)
            },
            Op::If {
                then_ops: vec![
                    Op::BorrowShared {
                        value: "a".into(),
                        token: "t".into(),
                        span: s(2)
                    },
                    Op::EndBorrow {
                        token: "t".into(),
                        span: s(3)
                    },
                ],
                else_ops: vec![Op::Use {
                    value: "a".into(),
                    span: s(4)
                }],
                span: s(1),
            },
            Op::Use {
                value: "a".into(),
                span: s(5)
            },
        ]),
        Ok(())
    );
}

#[test]
fn unbalanced_branch_state_is_explicit_error() {
    assert_error(
        &[Op::If {
            then_ops: vec![Op::EnterNoGc { span: s(2) }],
            else_ops: vec![],
            span: s(1),
        }],
        "CGO005",
        s(1),
    );
    assert_error(
        &[Op::If {
            then_ops: vec![Op::OpenRegion {
                region: "r".into(),
                span: s(2),
            }],
            else_ops: vec![],
            span: s(1),
        }],
        "CGO005",
        s(1),
    );
    assert_error(
        &[
            Op::EnterNoGc { span: s(0) },
            Op::If {
                then_ops: vec![Op::ExitNoGc { span: s(2) }, Op::EnterNoGc { span: s(3) }],
                else_ops: vec![Op::ExitNoGc { span: s(4) }, Op::EnterNoGc { span: s(5) }],
                span: s(1),
            },
        ],
        "CGO005",
        s(1),
    );
}

#[test]
fn maybe_live_managed_handle_still_blocks_region_close() {
    assert_error(
        &[
            Op::OpenRegion {
                region: "r".into(),
                span: s(0),
            },
            Op::If {
                then_ops: vec![Op::CreateManaged {
                    value: "h".into(),
                    region: "r".into(),
                    span: s(2),
                }],
                else_ops: vec![],
                span: s(1),
            },
            Op::CloseRegion {
                region: "r".into(),
                span: s(3),
            },
        ],
        "CGO003",
        s(3),
    );
}

#[test]
fn unsupported_loop_and_foreign_alias_are_reported() {
    for feature in ["loop", "foreign alias"] {
        assert_error(
            &[Op::Unsupported {
                feature: feature.into(),
                span: s(7),
            }],
            "CGO005",
            s(7),
        );
    }
}
