//! Authenticated source-owned v1. Copied TypedModule is deliberately not an adapter.
mod engine;
mod lower;
pub mod model;
use crate::TypeDiagnostic;
use cgx2_ownership::v1 as own;
use cgx2_syntax::{Module, Span};
pub use engine::{Evidence, Execution, Failure, Scalar};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_SOURCE: usize = 4 * 1024 * 1024;
/// Private fields and no deserializer/acceptance flag: all constructors check real source.
#[derive(Clone, Debug)]
pub struct OwnedProgram {
    source: String,
    source_sha256: String,
    module_sha256: String,
    module: Module,
    model: model::Model,
    ownership: Vec<own::Function>,
    summaries: Vec<own::FunctionSummary>,
}
impl OwnedProgram {
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn module_sha256(&self) -> &str {
        &self.module_sha256
    }
    pub fn model(&self) -> &model::Model {
        &self.model
    }
    pub fn ownership_functions(&self) -> &[own::Function] {
        &self.ownership
    }
    pub fn summaries(&self) -> &[own::FunctionSummary] {
        &self.summaries
    }
    /// Recheck source identity, typed IR, CFGs and complete summary provenance at each boundary.
    pub fn authenticate(&self) -> Result<(), Vec<TypeDiagnostic>> {
        let fresh = compile_source(&self.source)?;
        if fresh.source_sha256 != self.source_sha256
            || fresh.module_sha256 != self.module_sha256
            || fresh.module != self.module
            || fresh.model != self.model
            || fresh.ownership != self.ownership
            || fresh.summaries != self.summaries
        {
            return Err(vec![diagnostic(
                "CGT084",
                "forged or stale OwnedProgram evidence",
                self.module.span,
            )]);
        }
        Ok(())
    }
    pub fn authenticate_source(&self, source: &str) -> Result<(), Vec<TypeDiagnostic>> {
        if source != self.source {
            return Err(vec![diagnostic(
                "CGT084",
                "OwnedProgram source digest mismatch",
                self.module.span,
            )]);
        }
        self.authenticate()
    }
    pub fn execute(&self) -> Result<Execution, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(engine::run(&self.model))
    }
    pub fn execute_with_session(
        &self,
        session: cgx2_runtime::Session,
    ) -> Result<Execution, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(engine::run_with_session(&self.model, session))
    }
    /// Only an authenticated producer can publish an executable native IR literal.
    pub fn rust_data(&self) -> Result<String, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(self.model.rust())
    }
}
pub fn compile_source(source: &str) -> Result<OwnedProgram, Vec<TypeDiagnostic>> {
    if source.len() > MAX_SOURCE {
        return Err(vec![diagnostic(
            "CGT081",
            "ordinary source exceeds 4 MiB",
            Span { start: 0, end: 0 },
        )]);
    }
    let module = cgx2_syntax::parse_module(source).map_err(|ds| {
        ds.into_iter()
            .map(|d| diagnostic(d.code, d.message, d.span))
            .collect::<Vec<_>>()
    })?;
    let (model, ownership, summaries) = lower::lower(&module, source).map_err(|d| vec![d])?;
    Ok(OwnedProgram {
        source: source.into(),
        source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
        module_sha256: format!("{:x}", Sha256::digest(format!("{module:?}").as_bytes())),
        module,
        model,
        ownership,
        summaries,
    })
}
fn diagnostic(code: &'static str, message: impl Into<String>, span: Span) -> TypeDiagnostic {
    TypeDiagnostic::new(code, message.into(), span)
}
fn span(s: model::SourceSpan) -> Span {
    Span {
        start: s.start,
        end: s.end,
    }
}
fn source_span(s: Span) -> model::SourceSpan {
    model::SourceSpan {
        start: s.start,
        end: s.end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str =
        "record R:\n    x: i64\nfn main() -> i64:\n    let p = owned(R(x: 4))\n    p.x\n";
    #[test]
    fn stale_and_forged_evidence_never_executes() {
        let p = compile_source(SOURCE).unwrap();
        assert_eq!(
            p.authenticate_source(&SOURCE.replace("4", "5"))
                .unwrap_err()[0]
                .code,
            "CGT084"
        );
        let mut forged = p.clone();
        forged.model.functions[0].body.clear();
        assert_eq!(forged.execute().unwrap_err()[0].code, "CGT084");
        let mut forged = p.clone();
        forged.summaries[0].result = own::ResultMode::Owned {
            from_parameter: None,
        };
        assert_eq!(forged.rust_data().unwrap_err()[0].code, "CGT084");
        let mut forged = p;
        forged.ownership[0].blocks[0].operations.clear();
        assert_eq!(forged.execute().unwrap_err()[0].code, "CGT084");
        let mut forged = compile_source(SOURCE).unwrap();
        forged.model.layouts[0].fields[0].offset = usize::MAX;
        assert_eq!(forged.execute().unwrap_err()[0].span, forged.module.span);
        let mut forged = compile_source(SOURCE).unwrap();
        forged.source_sha256 = "00".repeat(32);
        assert_eq!(forged.execute().unwrap_err()[0].code, "CGT084");
    }
}
