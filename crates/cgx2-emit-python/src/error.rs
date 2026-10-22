use cgx2_syntax::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmitDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl EmitDiagnostic {
    pub(crate) fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
        }
    }
}
