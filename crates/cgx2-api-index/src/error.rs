use cgx2_sema::TypeDiagnostic;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexError {
    /// The supplied AST does not pass semantic checking.
    InvalidModule(Vec<TypeDiagnostic>),
    /// The summary differs from a fresh check of the supplied AST.
    MismatchedTypedModule,
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidModule(diagnostics) => write!(
                f,
                "module failed semantic checking ({} diagnostics)",
                diagnostics.len()
            ),
            Self::MismatchedTypedModule => write!(f, "typed module does not match syntax module"),
        }
    }
}

impl std::error::Error for IndexError {}
