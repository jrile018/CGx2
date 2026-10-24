pub use crate::v1::{
    AnalysisLimits, AnalysisReport, AnalysisStats, Diagnostic, LoanKind, Place, Projection,
    Retention,
};
use cgx2_syntax::Span;
pub(super) fn paths_overlap(a: &[Projection], b: &[Projection]) -> bool {
    Place {
        local: String::new(),
        projection: a.to_vec(),
    }
    .overlaps(&Place {
        local: String::new(),
        projection: b.to_vec(),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterMode {
    OwnedResult,
    Owned,
    Shared,
    Exclusive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResultMode {
    OwnedResult {
        from_parameter: Option<usize>,
    },
    Unit,
    /// `Some(i)` promises ownership derived from parameter i. `None` promises
    /// an independent, unregioned owner, not a hidden alias of a parameter.
    Owned {
        from_parameter: Option<usize>,
    },
    Loan {
        parameter: usize,
        projection: Vec<Projection>,
        kind: LoanKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSummary {
    pub name: String,
    pub parameters: Vec<ParameterMode>,
    pub result: ResultMode,
    pub retention: Retention,
}

/// Region-bound values are unique handles/aggregates, not freely aliased managed
/// graphs. Moves preserve storage; cross-region copying needs a future API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Storage {
    ConditionalOwned,
    Owned,
    Region(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Local {
    pub name: String,
    pub storage: Storage,
    pub span: Span,
}

/// Parameters occupy the first locals, in signature order. Other slots start
/// uninitialized. A dead slot can be initialized again (e.g. each loop iteration).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub summary: FunctionSummary,
    pub locals: Vec<Local>,
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
    pub operation: Operation,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallResult {
    OwnedResult(String),
    Unit,
    Owned(String),
    Loan(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    TryCreate(String),
    MoveResult {
        from: Place,
        to: String,
    },
    DropResult(Place),
    OpenRegion(String),
    CloseRegion(String),
    Create(String),
    Read(Place),
    Write(Place),
    Move {
        from: Place,
        to: String,
    },
    Drop(Place),
    /// Destroy the initialized remainder of a partially moved aggregate.
    DropRemainder(String),
    Borrow {
        place: Place,
        token: String,
        kind: LoanKind,
    },
    UseLoan {
        token: String,
        write: bool,
    },
    EndLoan(String),
    Call {
        function: String,
        arguments: Vec<Place>,
        result: CallResult,
        /// Required only for `UntilRelease`; represents a foreign owner handle.
        retention_owner: Option<String>,
    },
    ReleaseForeign(String),
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReturnValue {
    OwnedResult(Place),
    Unit,
    Owned(Place),
    Loan(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    MatchResult {
        from: Place,
        payload: String,
        ok: usize,
        err: usize,
    },
    Goto(usize),
    Branch {
        then_block: usize,
        else_block: usize,
    },
    Return(ReturnValue),
}
