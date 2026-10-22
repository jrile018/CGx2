use cgx2_syntax::Span;

/// Source span is a zero-based, half-open UTF-8 byte range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl TypeDiagnostic {
    pub(crate) fn new(code: &'static str, message: String, span: Span) -> Self {
        Self {
            code,
            message,
            span,
        }
    }
}
