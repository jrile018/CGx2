//! Authenticated source-owned v1. Copied TypedModule is deliberately not an adapter.
mod engine;
mod lower;
pub mod model;
pub mod model_v2;
use crate::TypeDiagnostic;
use cgx2_ownership::v1 as own;
use cgx2_syntax::{Module, Span};
pub use engine::{Evidence as EvidenceV2, Execution as ExecutionV2, Failure, Scalar};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const OWNED_RESULT_SCHEMA_VERSION: u32 = 2;
const MAX_SOURCE: usize = 4 * 1024 * 1024;
/// Private fields and no deserializer/acceptance flag: all constructors check real source.
#[derive(Clone, Debug)]
pub struct OwnedProgramV2 {
    ownership_v2: Vec<cgx2_ownership::v2::Function>,
    summaries_v2: Vec<cgx2_ownership::v2::FunctionSummary>,
    source: String,
    source_sha256: String,
    module_sha256: String,
    analysis_sha256: String,
    module: Module,
    model: model_v2::Model,
    ownership: Vec<own::Function>,
    summaries: Vec<own::FunctionSummary>,
}
impl OwnedProgramV2 {
    pub fn schema_version(&self) -> u32 {
        if self.model.as_v1().is_none()
            || self.ownership_v2.iter().any(|f| f.as_v1().is_none())
            || self.model.functions.iter().any(|f| contains_guard(&f.body))
        {
            OWNED_RESULT_SCHEMA_VERSION
        } else {
            SCHEMA_VERSION
        }
    }
    pub fn ownership_functions_v2(&self) -> &[cgx2_ownership::v2::Function] {
        &self.ownership_v2
    }
    pub fn summaries_v2(&self) -> &[cgx2_ownership::v2::FunctionSummary] {
        &self.summaries_v2
    }

    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn module_sha256(&self) -> &str {
        &self.module_sha256
    }
    /// Binds versioned CFG arms/callee provenance to the complete typed layout/effect body.
    pub fn analysis_sha256(&self) -> &str {
        &self.analysis_sha256
    }
    pub fn model(&self) -> &model_v2::Model {
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
        let fresh = compile_source_v2(&self.source)?;
        if fresh.source_sha256 != self.source_sha256
            || fresh.module_sha256 != self.module_sha256
            || fresh.analysis_sha256 != self.analysis_sha256
            || fresh.module != self.module
            || fresh.model != self.model
            || fresh.ownership != self.ownership
            || fresh.summaries != self.summaries
            || fresh.ownership_v2 != self.ownership_v2
            || fresh.summaries_v2 != self.summaries_v2
        {
            return Err(vec![diagnostic(
                "CGT084",
                "forged or stale OwnedProgramV2 evidence",
                self.module.span,
            )]);
        }
        Ok(())
    }
    pub fn authenticate_source(&self, source: &str) -> Result<(), Vec<TypeDiagnostic>> {
        if source != self.source {
            return Err(vec![diagnostic(
                "CGT084",
                "OwnedProgramV2 source digest mismatch",
                self.module.span,
            )]);
        }
        self.authenticate()
    }
    pub fn execute(&self) -> Result<ExecutionV2, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(engine::run(&self.model))
    }
    pub fn execute_with_session(
        &self,
        session: cgx2_runtime::Session,
    ) -> Result<ExecutionV2, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(engine::run_with_session(&self.model, session))
    }
    /// Only an authenticated producer can publish an executable native IR literal.
    pub fn rust_data(&self) -> Result<String, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(self.model.rust())
    }
}
pub fn compile_source_v2(source: &str) -> Result<OwnedProgramV2, Vec<TypeDiagnostic>> {
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
    let (model, ownership_v2, summaries_v2) = lower::lower(&module, source).map_err(|d| vec![d])?;
    let legacy: Option<Vec<_>> = if model.as_v1().is_none() {
        None
    } else {
        ownership_v2.iter().map(|f| f.as_v1()).collect()
    };
    let (ownership, summaries) = if let Some(functions) = legacy {
        let summaries: Vec<_> = functions.iter().map(|f| f.summary.clone()).collect();
        for f in &functions {
            let callees: Vec<_> = summaries
                .iter()
                .filter(|s| s.name != f.summary.name)
                .cloned()
                .collect();
            let report = own::analyze_function(f, &callees, source, own::AnalysisLimits::default());
            if !report.is_accepted() {
                return Err(report
                    .diagnostics
                    .into_iter()
                    .map(|d| diagnostic(d.code, d.message, d.span))
                    .collect());
            }
        }
        (functions, summaries)
    } else {
        (vec![], vec![])
    };
    Ok(OwnedProgramV2 {
        analysis_sha256: format!(
            "{:x}",
            Sha256::digest(
                format!("owned-analysis-v2:{source}:{model:?}:{ownership_v2:?}:{summaries_v2:?}")
                    .as_bytes()
            )
        ),
        ownership_v2,
        summaries_v2,
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

fn contains_guard(body: &[model_v2::Statement]) -> bool {
    body.iter().any(|s| match &s.kind {
        model_v2::StatementKind::NoGc { .. } => true,
        model_v2::StatementKind::If {
            then_body,
            else_body,
            ..
        } => contains_guard(then_body) || contains_guard(else_body),
        model_v2::StatementKind::MatchResult {
            ok_body, err_body, ..
        } => contains_guard(ok_body) || contains_guard(err_body),
        model_v2::StatementKind::MatchError { arms, .. } => arms.iter().any(|a| contains_guard(a)),
        _ => false,
    })
}

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
    pub fn schema_version(&self) -> u32 {
        SCHEMA_VERSION
    }
    /// Authenticated upgrade; no model flags or unchecked summary imports.
    pub fn upgrade_v2(&self) -> Result<OwnedProgramV2, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        compile_source_v2(&self.source)
    }

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
        Ok(engine::run(&compile_source_v2(&self.source)?.model).into())
    }
    pub fn execute_with_session(
        &self,
        session: cgx2_runtime::Session,
    ) -> Result<Execution, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(engine::run_with_session(&compile_source_v2(&self.source)?.model, session).into())
    }
    /// Only an authenticated producer can publish an executable native IR literal.
    pub fn rust_data(&self) -> Result<String, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(self.model.rust())
    }
}
pub fn compile_source(source: &str) -> Result<OwnedProgram, Vec<TypeDiagnostic>> {
    let fresh = compile_source_v2(source)?;
    let model = fresh.model.as_v1().ok_or_else(|| {
        vec![diagnostic(
            "CGT081",
            "schema-2 source requires compile_source_v2",
            fresh.module.span,
        )]
    })?;
    Ok(OwnedProgram {
        source: fresh.source,
        source_sha256: fresh.source_sha256,
        module_sha256: fresh.module_sha256,
        module: fresh.module,
        model,
        ownership: fresh.ownership,
        summaries: fresh.summaries,
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Evidence {
    pub allocations: usize,
    pub releases: usize,
    pub live_bytes: usize,
    pub allocated_bytes: usize,
    pub released_bytes: usize,
    pub peak_live_bytes: usize,
    pub reads: usize,
    pub writes: usize,
}
impl Evidence {
    pub fn json(&self) -> String {
        format!(
            "{{\"schema\":1,\"allocations\":{},\"releases\":{},\"live_bytes\":{},\"allocated_bytes\":{},\"released_bytes\":{},\"peak_live_bytes\":{},\"reads\":{},\"writes\":{}}}",
            self.allocations,
            self.releases,
            self.live_bytes,
            self.allocated_bytes,
            self.released_bytes,
            self.peak_live_bytes,
            self.reads,
            self.writes
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    pub result: Result<Scalar, Failure>,
    pub evidence: Evidence,
}
impl From<ExecutionV2> for Execution {
    fn from(value: ExecutionV2) -> Self {
        let e = value.evidence;
        Self {
            result: value.result,
            evidence: Evidence {
                allocations: e.allocations,
                releases: e.releases,
                live_bytes: e.live_bytes,
                allocated_bytes: e.allocated_bytes,
                released_bytes: e.released_bytes,
                peak_live_bytes: e.peak_live_bytes,
                reads: e.reads,
                writes: e.writes,
            },
        }
    }
}

/// Internal hybrid producer adapter. Managed functions are replaced only by
/// copied scalar declarations; no managed handle is sent to the affine analyzer.
pub(crate) fn analyze_projection(
    module: &Module,
    source: &str,
) -> Result<(model_v2::Model, String), Vec<TypeDiagnostic>> {
    let (model, functions, summaries) = lower::lower(module, source).map_err(|d| vec![d])?;
    // lower::lower requires every v2 worklist report to be accepted.
    let digest = format!(
        "{:x}",
        Sha256::digest(
            format!("hybrid-owned-v1:{source}:{module:?}:{model:?}:{functions:?}:{summaries:?}")
                .as_bytes()
        )
    );
    Ok((model, digest))
}
pub use engine::{run_with_bridge, ManagedBridge};
pub(crate) fn projection_rust(model: &model_v2::Model) -> String {
    model.rust()
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

    #[test]
    fn stale_conditional_arm_and_callee_provenance_rejects() {
        let source = include_str!("../../cgx2-syntax/owned-result-evidence/fixtures/success.cg");
        let p = compile_source_v2(source).unwrap();
        let mut forged = p.clone();
        let summary = forged
            .summaries_v2
            .iter_mut()
            .find(|s| s.name == "pass")
            .unwrap();
        summary.result = cgx2_ownership::v2::ResultMode::OwnedResult {
            from_parameter: None,
        };
        assert_eq!(forged.execute().unwrap_err()[0].code, "CGT084");
        let mut forged = p.clone();
        for f in &mut forged.ownership_v2 {
            for b in &mut f.blocks {
                if let cgx2_ownership::v2::Terminator::MatchResult { ok, err, .. } =
                    &mut b.terminator
                {
                    *err = *ok;
                }
            }
        }
        assert_eq!(forged.rust_data().unwrap_err()[0].code, "CGT084");
        let mut forged = p;
        forged.model.functions[1].ownership_parameters.reverse();
        assert_eq!(forged.execute().unwrap_err()[0].code, "CGT084");
    }
}
