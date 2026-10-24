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
    if cgx2_sema::ordinary_memory::profile(&module) {
        return cgx2_sema::ordinary_memory::compile_source(source)
            .map(|p| p.typed().clone())
            .map_err(|ds| {
                ds.into_iter()
                    .map(|d| CheckDiagnostic {
                        code: d.code.into(),
                        message: d.message,
                        span: d.span,
                    })
                    .collect()
            });
    }
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
    if cgx2_sema::default_managed::profile(&module) {
        if let Err(ds) = cgx2_sema::default_managed::compile_source(source) {
            // Literal escape portability is a target preflight, as in the
            // existing source checker. Execution/emission still authenticate it.
            let ds = ds
                .into_iter()
                .filter(|d| d.code != "CGRS003")
                .collect::<Vec<_>>();
            if !ds.is_empty() {
                return Err(ds
                    .into_iter()
                    .map(|d| CheckDiagnostic {
                        code: d.code.into(),
                        message: d.message,
                        span: d.span,
                    })
                    .collect::<Vec<_>>());
            }
        }
    }
    Ok(typed)
}
