use cgx2_sema::{check_module, TypedModule};
use cgx2_syntax::{parse_module, Span};

/// A parser, scalar, memory-source, or ownership error in zero-based, half-open UTF-8 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckDiagnostic {
    pub code: String,
    pub message: String,
    pub span: Span,
}

/// Parse source and check scalar semantics or the bounded memory-source subset without executing it.
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
    let typed = check_module(&module).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| CheckDiagnostic {
                code: error.code.to_owned(),
                message: error.message,
                span: error.span,
            })
            .collect::<Vec<_>>()
    })?;
    if let Some(function) = module.functions.iter().find(|function| {
        function
            .foreign
            .as_ref()
            .is_some_and(|binding| binding.route == "native")
    }) {
        return Err(vec![CheckDiagnostic {
            code: "CGT017".into(),
            message: "native C calls require an explicit schema 2 package and pinned binding lock"
                .into(),
            span: function.span,
        }]);
    }
    Ok(typed)
}
