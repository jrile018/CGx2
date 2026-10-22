use cgx2_syntax::Span;

/// Explicit, source-spanned operations. Names are unique within one program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    OpenRegion {
        region: String,
        span: Span,
    },
    CreateOwned {
        value: String,
        span: Span,
    },
    Use {
        value: String,
        span: Span,
    },
    Move {
        from: String,
        to: String,
        span: Span,
    },
    BorrowShared {
        value: String,
        token: String,
        span: Span,
    },
    BorrowMut {
        value: String,
        token: String,
        span: Span,
    },
    EndBorrow {
        token: String,
        span: Span,
    },
    Promote {
        from: String,
        to: String,
        region: String,
        span: Span,
    },
    CopyToOwned {
        from: String,
        to: String,
        span: Span,
    },
    CreateManaged {
        value: String,
        region: String,
        span: Span,
    },
    DropValue {
        value: String,
        span: Span,
    },
    CloseRegion {
        region: String,
        span: Span,
    },
    EnterNoGc {
        span: Span,
    },
    ExitNoGc {
        span: Span,
    },
    Collect {
        region: String,
        span: Span,
    },
    If {
        then_ops: Vec<Op>,
        else_ops: Vec<Op>,
        span: Span,
    },
    /// A lowering pass can preserve a construct this checker cannot model.
    Unsupported {
        feature: String,
        span: Span,
    },
}

impl Op {
    pub fn span(&self) -> Span {
        match self {
            Self::OpenRegion { span, .. }
            | Self::CreateOwned { span, .. }
            | Self::Use { span, .. }
            | Self::Move { span, .. }
            | Self::BorrowShared { span, .. }
            | Self::BorrowMut { span, .. }
            | Self::EndBorrow { span, .. }
            | Self::Promote { span, .. }
            | Self::CopyToOwned { span, .. }
            | Self::CreateManaged { span, .. }
            | Self::DropValue { span, .. }
            | Self::CloseRegion { span, .. }
            | Self::EnterNoGc { span }
            | Self::ExitNoGc { span }
            | Self::Collect { span, .. }
            | Self::If { span, .. }
            | Self::Unsupported { span, .. } => *span,
        }
    }
}
