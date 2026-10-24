//! Additive ownership CFG with a live owner on both transfer outcomes.
//!
//! Lowering creates explicit edge blocks before invoking the same bounded
//! definite/possible-liveness worklist as v2. An owner-bearing transfer carrier
//! cannot be observed using ordinary owner operations or v2's Ok-only match.
use crate::v2;
use cgx2_syntax::Span;
use std::collections::BTreeSet;

pub use v2::{
    AnalysisLimits, AnalysisReport, AnalysisStats, CallResult, Diagnostic, LoanKind, Local,
    ParameterMode, Place, Projection, ResultMode, Retention, ReturnValue, Storage,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSummary {
    pub ordinary: v2::FunctionSummary,
    pub region_parameters: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub summary: FunctionSummary,
    pub locals: Vec<Local>,
    pub transfer_carriers: BTreeSet<String>,
    pub blocks: Vec<Block>,
    pub entry: usize,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub operations: Vec<SpannedOperation>,
    pub terminator: Terminator,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpannedOperation {
    pub op: Operation,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Ordinary(v2::Operation),
    /// Consumes the original place into an opaque affine recovery carrier.
    /// No source owner is duplicated or marked dead only on the success edge.
    PrepareTransfer {
        from: Place,
        carrier: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Ordinary(v2::Terminator),
    MatchTransfer {
        carrier: String,
        success: String,
        failure: String,
        ok: usize,
        err: usize,
    },
}

fn rejected(message: &str, span: Span, source: &str, limits: AnalysisLimits) -> AnalysisReport {
    // Let the existing validator construct the canonical complete diagnostic
    // report and validate the UTF-8 span, rather than inventing an acceptance bit.
    let f = v2::Function {
        summary: v2::FunctionSummary {
            name: "$invalid-v3".into(),
            parameters: vec![],
            result: ResultMode::Unit,
            retention: Retention::Local,
        },
        locals: vec![],
        entry: 0,
        span,
        blocks: vec![v2::Block {
            operations: vec![v2::SpannedOperation {
                operation: v2::Operation::Unsupported(message.into()),
                span,
            }],
            terminator: v2::Terminator::Return(ReturnValue::Unit),
            span,
        }],
    };
    v2::analyze_function(&f, &[], source, limits)
}

fn result_mentions(result: &CallResult, carriers: &BTreeSet<String>) -> bool {
    match result {
        CallResult::Unit => false,
        CallResult::Owned(n) | CallResult::OwnedResult(n) | CallResult::Loan(n) => {
            carriers.contains(n)
        }
    }
}
fn operation_mentions(op: &v2::Operation, carriers: &BTreeSet<String>) -> bool {
    use v2::Operation as O;
    let place = |p: &Place| carriers.contains(&p.local);
    match op {
        O::TryCreate(n)
        | O::OpenRegion(n)
        | O::CloseRegion(n)
        | O::Create(n)
        | O::DropRemainder(n)
        | O::EndLoan(n)
        | O::ReleaseForeign(n) => carriers.contains(n),
        O::Read(p) | O::Write(p) | O::Drop(p) | O::DropResult(p) => place(p),
        O::Move { from, to } | O::MoveResult { from, to } => place(from) || carriers.contains(to),
        O::Borrow {
            place: p, token, ..
        } => place(p) || carriers.contains(token),
        O::UseLoan { token, .. } => carriers.contains(token),
        O::Call {
            arguments,
            result,
            retention_owner,
            ..
        } => {
            arguments.iter().any(place)
                || result_mentions(result, carriers)
                || retention_owner
                    .as_ref()
                    .is_some_and(|n| carriers.contains(n))
        }
        O::Unsupported(_) => false,
    }
}
fn terminator_mentions(t: &v2::Terminator, carriers: &BTreeSet<String>) -> bool {
    match t {
        v2::Terminator::MatchResult { from, payload, .. } => {
            carriers.contains(&from.local) || carriers.contains(payload)
        }
        v2::Terminator::Return(ReturnValue::Owned(p) | ReturnValue::OwnedResult(p)) => {
            carriers.contains(&p.local)
        }
        v2::Terminator::Return(ReturnValue::Loan(n)) => carriers.contains(n),
        _ => false,
    }
}
fn limit_report(source: &str, span: Span) -> AnalysisReport {
    let span = if span.start <= span.end && source.get(span.start..span.end).is_some() {
        span
    } else {
        Span { start: 0, end: 0 }
    };
    AnalysisReport {
        complete: false,
        diagnostics: vec![Diagnostic {
            code: "CGO106",
            message: "v3 input or expanded transfer CFG exceeds analysis limits".into(),
            span,
            related: vec![],
        }],
        stats: AnalysisStats::default(),
    }
}

pub fn analyze_function(
    function: &Function,
    summaries: &[FunctionSummary],
    source: &str,
    limits: AnalysisLimits,
) -> AnalysisReport {
    let f = function;
    let reject = |message: &str, span: Span| rejected(message, span, source, limits);
    let operations = f
        .blocks
        .iter()
        .fold(0usize, |n, b| n.saturating_add(b.operations.len()));
    let transfers = f
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, Terminator::MatchTransfer { .. }))
        .count();
    if source.len() > limits.max_source_bytes
        || summaries.len() > limits.max_summaries
        || f.blocks.len().saturating_add(transfers.saturating_mul(2)) > limits.max_blocks
        || f.locals.len() > limits.max_locals
        || operations > limits.max_operations
        || f.transfer_carriers.len() > limits.max_locals
        || f.locals
            .iter()
            .any(|l| l.name.len() > limits.max_name_bytes)
        || f.transfer_carriers
            .iter()
            .any(|n| n.len() > limits.max_name_bytes)
        || f.summary.region_parameters.len() > limits.max_locals
        || f.summary
            .region_parameters
            .iter()
            .any(|n| n.len() > limits.max_name_bytes)
    {
        return limit_report(source, f.span);
    }
    let mut regions = BTreeSet::new();
    if f.summary
        .region_parameters
        .iter()
        .any(|r| r.is_empty() || !regions.insert(r))
    {
        return reject("invalid or repeated region quantifier", f.span);
    }
    for carrier in &f.transfer_carriers {
        if !f
            .locals
            .iter()
            .any(|l| l.name == *carrier && l.storage == Storage::Owned)
            || f.locals
                .iter()
                .take(f.summary.ordinary.parameters.len())
                .any(|l| l.name == *carrier)
        {
            return reject("transfer carrier must be a local opaque owner", f.span);
        }
    }
    let mut blocks = Vec::new();
    let mut edges = Vec::new();
    for block in &f.blocks {
        let mut operations = Vec::new();
        for operation in &block.operations {
            let op = match &operation.op {
                Operation::Ordinary(op) => {
                    if operation_mentions(op, &f.transfer_carriers) {
                        return reject("ordinary operation observes a transfer carrier", f.span);
                    }
                    op.clone()
                }
                Operation::PrepareTransfer { from, carrier } => {
                    if !f.transfer_carriers.contains(carrier)
                        || f.transfer_carriers.contains(&from.local)
                    {
                        return reject("invalid transfer preparation", f.span);
                    }
                    v2::Operation::Move {
                        from: from.clone(),
                        to: carrier.clone(),
                    }
                }
            };
            operations.push(v2::SpannedOperation {
                operation: op,
                span: operation.span,
            });
        }
        let terminator = match &block.terminator {
            Terminator::Ordinary(t) => {
                if terminator_mentions(t, &f.transfer_carriers) {
                    return reject("ordinary terminator observes a transfer carrier", f.span);
                }
                t.clone()
            }
            Terminator::MatchTransfer {
                carrier,
                success,
                failure,
                ok,
                err,
            } => {
                if !f.transfer_carriers.contains(carrier)
                    || f.transfer_carriers.contains(success)
                    || f.transfer_carriers.contains(failure)
                    || !f
                        .locals
                        .iter()
                        .any(|l| l.name == *success && matches!(l.storage, Storage::Region(_)))
                    || !f
                        .locals
                        .iter()
                        .any(|l| l.name == *failure && l.storage == Storage::Owned)
                    || *ok >= f.blocks.len()
                    || *err >= f.blocks.len()
                {
                    return reject("invalid transfer outcome places or edges", f.span);
                }
                let place = Place::local(carrier);
                let base = f.blocks.len() + edges.len();
                edges.push(v2::Block {
                    operations: vec![
                        v2::SpannedOperation {
                            operation: v2::Operation::Drop(place.clone()),
                            span: block.span,
                        },
                        v2::SpannedOperation {
                            operation: v2::Operation::Create(success.clone()),
                            span: block.span,
                        },
                    ],
                    terminator: v2::Terminator::Goto(*ok),
                    span: block.span,
                });
                edges.push(v2::Block {
                    operations: vec![v2::SpannedOperation {
                        operation: v2::Operation::Move {
                            from: place,
                            to: failure.clone(),
                        },
                        span: block.span,
                    }],
                    terminator: v2::Terminator::Goto(*err),
                    span: block.span,
                });
                v2::Terminator::Branch {
                    then_block: base,
                    else_block: base + 1,
                }
            }
        };
        blocks.push(v2::Block {
            operations,
            terminator,
            span: block.span,
        });
        if blocks.len() + edges.len() > limits.max_blocks {
            return limit_report(source, f.span);
        }
    }
    blocks.extend(edges);
    let lowered = v2::Function {
        summary: f.summary.ordinary.clone(),
        locals: f.locals.clone(),
        blocks,
        entry: f.entry,
        span: f.span,
    };
    let summaries = summaries
        .iter()
        .map(|s| s.ordinary.clone())
        .collect::<Vec<_>>();
    v2::analyze_function(&lowered, &summaries, source, limits)
}
