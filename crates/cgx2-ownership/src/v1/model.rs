use cgx2_syntax::Span;

/// Paths are structural, with no pointer dereference or implicit aliasing.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Projection {
    Field(String),
    Tuple(u32),
    Index(u64),
    /// May refer to any element; it cannot be used for an owned partial move.
    DynamicIndex,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Place {
    pub local: String,
    pub projection: Vec<Projection>,
}

impl Place {
    pub fn local(name: impl Into<String>) -> Self {
        Self {
            local: name.into(),
            projection: Vec::new(),
        }
    }

    pub fn project(mut self, projection: Projection) -> Self {
        self.projection.push(projection);
        self
    }

    /// Prefixes overlap; known sibling fields/elements are disjoint. Mixed
    /// projection kinds and dynamic indices conservatively overlap.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.local == other.local && paths_overlap(&self.projection, &other.projection)
    }
}

pub(super) fn paths_overlap(a: &[Projection], b: &[Projection]) -> bool {
    for (a, b) in a.iter().zip(b) {
        if a == b {
            continue;
        }
        return !matches!(
            (a, b),
            (Projection::Field(_), Projection::Field(_))
                | (Projection::Tuple(_), Projection::Tuple(_))
                | (Projection::Index(_), Projection::Index(_))
        );
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoanKind {
    Shared,
    Exclusive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterMode {
    Owned,
    Shared,
    Exclusive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResultMode {
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
pub enum Retention {
    /// A checked local CGx2 body, with no hidden retention.
    Local,
    /// Foreign code releases all borrowed arguments before returning.
    CallOnly,
    /// Foreign code retains these borrowed arguments until explicit release.
    UntilRelease { parameters: Vec<usize> },
    /// No authenticated lifetime contract. Always rejected at a call.
    Unknown,
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
    Unit,
    Owned(String),
    Loan(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
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
    Unit,
    Owned(Place),
    Loan(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Goto(usize),
    Branch {
        then_block: usize,
        else_block: usize,
    },
    Return(ReturnValue),
}

/// All limits must be positive. Exhaustion produces CGO106 and never acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalysisLimits {
    pub max_blocks: usize,
    pub max_operations: usize,
    pub max_locals: usize,
    pub max_summaries: usize,
    pub max_projection_depth: usize,
    pub max_name_bytes: usize,
    /// Operations, terminators and edge propagations across all revisits.
    pub max_steps: usize,
    /// Entries across *all* cached block states and the current working state.
    pub max_state_entries: usize,
    pub max_source_bytes: usize,
}

impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            max_blocks: 256,
            max_operations: 4096,
            max_locals: 256,
            max_summaries: 256,
            max_projection_depth: 16,
            max_name_bytes: 256,
            max_steps: 10_000,
            max_state_entries: 65_536,
            max_source_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    /// Always a validated half-open UTF-8 byte range; malformed input spans
    /// themselves produce CGO105 at 0..0.
    pub span: Span,
    pub related: Vec<Span>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalysisStats {
    pub steps: usize,
    pub block_visits: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisReport {
    /// False for a resource limit; rejected programs can still be complete.
    pub complete: bool,
    /// Deterministic first error. No subsequent state is treated as valid.
    pub diagnostics: Vec<Diagnostic>,
    pub stats: AnalysisStats,
}

impl AnalysisReport {
    pub fn is_accepted(&self) -> bool {
        self.complete && self.diagnostics.is_empty()
    }
}
