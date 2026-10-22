use cgx2_syntax::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnershipDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}
