use cgx2_sema::{check_module, TypedModule};
use cgx2_syntax::{parse_module, Span};

/// A parser or baseline semantic error in zero-based, half-open UTF-8 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckDiagnostic {
    pub code: String,
    pub message: String,
    pub span: Span,
}

/// Parse source and check the Wave 2 scalar/type subset without executing it.
pub fn check_source(source: &str) -> Result<TypedModule, Vec<CheckDiagnostic>> {
    let module = parse_module(source).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| CheckDiagnostic {
                code: error.code.to_owned(),
                message: error.message,
                span: error.span,
            })
            .collect::<Vec<_>>()
    })?;
    check_module(&module).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| CheckDiagnostic {
                code: error.code.to_owned(),
                message: error.message,
                span: error.span,
            })
            .collect::<Vec<_>>()
    })
}
