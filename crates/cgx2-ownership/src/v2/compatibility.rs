//! Lossless schema-1 projection only. Conditional shapes never become v1 owners/Unit.
use super::*;
use crate::v1 as old;
impl FunctionSummary {
    pub fn as_v1(&self) -> Option<old::FunctionSummary> {
        Some(old::FunctionSummary {
            name: self.name.clone(),
            parameters: self
                .parameters
                .iter()
                .map(|p| {
                    Some(match p {
                        ParameterMode::Owned => old::ParameterMode::Owned,
                        ParameterMode::Shared => old::ParameterMode::Shared,
                        ParameterMode::Exclusive => old::ParameterMode::Exclusive,
                        ParameterMode::OwnedResult => return None,
                    })
                })
                .collect::<Option<_>>()?,
            result: match &self.result {
                ResultMode::Unit => old::ResultMode::Unit,
                ResultMode::Owned { from_parameter } => old::ResultMode::Owned {
                    from_parameter: *from_parameter,
                },
                ResultMode::Loan {
                    parameter,
                    projection,
                    kind,
                } => old::ResultMode::Loan {
                    parameter: *parameter,
                    projection: projection.clone(),
                    kind: *kind,
                },
                ResultMode::OwnedResult { .. } => return None,
            },
            retention: self.retention.clone(),
        })
    }
}
impl Function {
    pub fn as_v1(&self) -> Option<old::Function> {
        Some(old::Function {
            summary: self.summary.as_v1()?,
            locals: self
                .locals
                .iter()
                .map(|l| {
                    Some(old::Local {
                        name: l.name.clone(),
                        span: l.span,
                        storage: match &l.storage {
                            Storage::Owned => old::Storage::Owned,
                            Storage::Region(r) => old::Storage::Region(r.clone()),
                            Storage::ConditionalOwned => return None,
                        },
                    })
                })
                .collect::<Option<_>>()?,
            blocks: self
                .blocks
                .iter()
                .map(|b| {
                    Some(old::Block {
                        span: b.span,
                        operations: b
                            .operations
                            .iter()
                            .map(|o| {
                                Some(old::SpannedOperation {
                                    span: o.span,
                                    operation: match &o.operation {
                                        Operation::TryCreate(_)
                                        | Operation::MoveResult { .. }
                                        | Operation::DropResult(_) => return None,
                                        Operation::OpenRegion(n) => {
                                            old::Operation::OpenRegion(n.clone())
                                        }
                                        Operation::CloseRegion(n) => {
                                            old::Operation::CloseRegion(n.clone())
                                        }
                                        Operation::Create(n) => old::Operation::Create(n.clone()),
                                        Operation::Read(p) => old::Operation::Read(p.clone()),
                                        Operation::Write(p) => old::Operation::Write(p.clone()),
                                        Operation::Move { from, to } => old::Operation::Move {
                                            from: from.clone(),
                                            to: to.clone(),
                                        },
                                        Operation::Drop(p) => old::Operation::Drop(p.clone()),
                                        Operation::DropRemainder(n) => {
                                            old::Operation::DropRemainder(n.clone())
                                        }
                                        Operation::Borrow { place, token, kind } => {
                                            old::Operation::Borrow {
                                                place: place.clone(),
                                                token: token.clone(),
                                                kind: *kind,
                                            }
                                        }
                                        Operation::UseLoan { token, write } => {
                                            old::Operation::UseLoan {
                                                token: token.clone(),
                                                write: *write,
                                            }
                                        }
                                        Operation::EndLoan(n) => old::Operation::EndLoan(n.clone()),
                                        Operation::Call {
                                            function,
                                            arguments,
                                            result,
                                            retention_owner,
                                        } => old::Operation::Call {
                                            function: function.clone(),
                                            arguments: arguments.clone(),
                                            retention_owner: retention_owner.clone(),
                                            result: match result {
                                                CallResult::Unit => old::CallResult::Unit,
                                                CallResult::Owned(n) => {
                                                    old::CallResult::Owned(n.clone())
                                                }
                                                CallResult::Loan(n) => {
                                                    old::CallResult::Loan(n.clone())
                                                }
                                                CallResult::OwnedResult(_) => return None,
                                            },
                                        },
                                        Operation::ReleaseForeign(n) => {
                                            old::Operation::ReleaseForeign(n.clone())
                                        }
                                        Operation::Unsupported(n) => {
                                            old::Operation::Unsupported(n.clone())
                                        }
                                    },
                                })
                            })
                            .collect::<Option<_>>()?,
                        terminator: match &b.terminator {
                            Terminator::MatchResult { .. } => return None,
                            Terminator::Goto(i) => old::Terminator::Goto(*i),
                            Terminator::Branch {
                                then_block,
                                else_block,
                            } => old::Terminator::Branch {
                                then_block: *then_block,
                                else_block: *else_block,
                            },
                            Terminator::Return(v) => old::Terminator::Return(match v {
                                ReturnValue::Unit => old::ReturnValue::Unit,
                                ReturnValue::Owned(p) => old::ReturnValue::Owned(p.clone()),
                                ReturnValue::Loan(n) => old::ReturnValue::Loan(n.clone()),
                                ReturnValue::OwnedResult(_) => return None,
                            }),
                        },
                    })
                })
                .collect::<Option<_>>()?,
            entry: self.entry,
            span: self.span,
        })
    }
}
