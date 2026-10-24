//! Authenticated copied-scalar bridge; affine values never become managed ghosts.
use super::{diagnostic, ManagedProgramV1};
use crate::{ordinary_owned, TypeDiagnostic};
use cgx2_syntax::{Expr, ExprKind, Function, Module, Path, Span, Stmt, StmtKind, TypeSyntax};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod owned_bridge {
    pub use crate::ordinary_owned::{
        model_v2::Model, run_with_bridge, EvidenceV2, Failure, ManagedBridge, Scalar,
    };
    pub fn source_span(
        loc: cgx2_runtime::default_managed::model::Loc,
    ) -> crate::ordinary_owned::model::SourceSpan {
        crate::ordinary_owned::model::SourceSpan {
            start: loc.start,
            end: loc.end,
        }
    }
}
#[path = "hybrid_engine.rs"]
mod execution;
pub use execution::HybridExecution;
#[derive(Clone, Debug)]
pub struct HybridProgramV1 {
    source: String,
    source_sha256: String,
    analysis_sha256: String,
    owned_model: ordinary_owned::model_v2::Model,
    managed: ManagedProgramV1,
    bridges: Vec<(String, usize)>,
}
pub fn is_hybrid(m: &Module) -> bool {
    if !m.functions.iter().any(|f| affine(m, f)) {
        return false;
    }
    if super::explicit(m) {
        return true;
    }
    // An ordinary heap helper needs the same Session bridge even without any
    // explicit managed builtin. Preserve scalar-only owned initializers.
    let heap = std::cell::Cell::new(false);
    for f in m.functions.iter().filter(|f| !affine(m, f)) {
        cgx2_syntax::visit_stmts(&f.body, &mut |_| {}, &mut |e| match &e.kind {
            ExprKind::Collection(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Index { .. }
            | ExprKind::Constructor { .. } => heap.set(true),
            ExprKind::Call { callee, .. } if matches!(&callee.kind, ExprKind::Name(p) if !m.functions.iter().any(|f| f.name == p.segments.join(".")) && matches!(p.segments.join(".").as_str(), "length" | "get" | "updated" | "append" | "array" | "list" | "concat" | "slice" | "parse_i64" | "show" | "arg")) => {
                heap.set(true)
            }
            _ => {}
        });
    }
    heap.get()
}
pub(super) fn affine(module: &Module, f: &Function) -> bool {
    let mut m = module.clone();
    for record in &mut m.records {
        record.fields.clear();
    }
    m.variants.clear();
    m.statements.clear();
    for other in &mut m.functions {
        if other.name == f.name {
            *other = f.clone();
        } else {
            other.parameters.clear();
            other.return_type = None;
            other.body.clear();
        }
    }
    // Guards alone are not affine. Only wrapper types and actual owner/loan operations select this bridge.
    fn strip(body: &mut Vec<Stmt>) {
        let mut out = vec![];
        for mut s in std::mem::take(body) {
            match &mut s.kind {
                StmtKind::NoGc { body, .. } => {
                    strip(body);
                    out.append(body);
                    continue;
                }
                StmtKind::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    strip(then_body);
                    strip(else_body);
                }
                StmtKind::Match { arms, .. } => {
                    for a in arms {
                        strip(&mut a.body)
                    }
                }
                StmtKind::For { body, .. } => strip(body),
                _ => {}
            }
            out.push(s);
        }
        *body = out;
    }
    strip(
        &mut m
            .functions
            .iter_mut()
            .find(|g| g.name == f.name)
            .unwrap()
            .body,
    );
    cgx2_syntax::ordinary_owned_span(&m).is_some()
}
fn scalar(t: &TypeSyntax) -> bool {
    t.arguments.is_empty() && matches!(t.path.segments.join(".").as_str(), "i64" | "Bool" | "Unit")
}
fn stub(f: &Function) -> Function {
    let mut f = f.clone();
    f.effects = None;
    f.capabilities = None;
    f.requires.clear();
    f.ensures.clear();
    let span = f.span;
    let kind = match f
        .return_type
        .as_ref()
        .map(|t| t.path.segments.join("."))
        .as_deref()
    {
        Some("i64") => StmtKind::Expr(Expr {
            kind: ExprKind::Integer("0".into()),
            span,
        }),
        Some("Bool") => StmtKind::Expr(Expr {
            kind: ExprKind::Boolean(false),
            span,
        }),
        _ => StmtKind::Return(None),
    };
    f.body = vec![Stmt { kind, span }];
    f
}
impl HybridProgramV1 {
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn analysis_sha256(&self) -> &str {
        &self.analysis_sha256
    }
    pub fn managed(&self) -> &ManagedProgramV1 {
        &self.managed
    }
    pub fn entry_span(&self) -> Span {
        let span = self.owned_model.functions[self.owned_model.entry].span;
        Span {
            start: span.start,
            end: span.end,
        }
    }
    pub fn authenticate(&self) -> Result<(), Vec<TypeDiagnostic>> {
        let fresh = compile_source(&self.source)?;
        if fresh.analysis_sha256 != self.analysis_sha256
            || fresh.source_sha256 != self.source_sha256
            || fresh.owned_model != self.owned_model
            || fresh.bridges != self.bridges
            || fresh.managed.analysis_sha256() != self.managed.analysis_sha256()
        {
            return Err(vec![diagnostic(
                "forged mixed execution envelope",
                Span { start: 0, end: 0 },
            )]);
        }
        self.managed.authenticate()
    }
    pub fn owned_rust_data(&self) -> Result<String, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(ordinary_owned::projection_rust(&self.owned_model))
    }
    pub fn bridges(&self) -> &[(String, usize)] {
        &self.bridges
    }
    pub fn execute(&self) -> Result<HybridExecution, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(execution::run(
            &self.owned_model,
            self.managed.program(),
            &self.bridges,
        ))
    }
    pub fn execute_with_session(
        &self,
        session: cgx2_runtime::Session,
    ) -> Result<HybridExecution, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(execution::run_with_session(
            &self.owned_model,
            self.managed.program(),
            &self.bridges,
            session,
        ))
    }
}
pub fn compile_source(source: &str) -> Result<HybridProgramV1, Vec<TypeDiagnostic>> {
    let original = cgx2_syntax::parse_module(source).map_err(|ds| {
        ds.into_iter()
            .map(|d| TypeDiagnostic {
                code: d.code,
                message: d.message,
                span: d.span,
            })
            .collect::<Vec<_>>()
    })?;
    if !is_hybrid(&original) {
        return Err(vec![diagnostic(
            "source does not select mixed owned/managed execution",
            original.span,
        )]);
    }
    if !original.imports.is_empty() || original.name.is_some() {
        return Err(vec![diagnostic(
            "mixed ownership packages require the later owned-package provenance gate",
            original.span,
        )]);
    }
    let owned: BTreeSet<_> = original
        .functions
        .iter()
        .filter(|f| f.name == "main" || affine(&original, f))
        .map(|f| f.name.clone())
        .collect();
    let mut managed_module = original.clone();
    managed_module
        .functions
        .retain(|f| !owned.contains(&f.name));
    // The managed closure must be independent of affine callees. No placeholder
    // implementation can stand in for an ownership operation during execution.
    let mut errors = vec![];
    for f in &managed_module.functions {
        cgx2_syntax::visit_stmts(&f.body, &mut |_| {}, &mut |e| {
            if let ExprKind::Call { callee, .. } = &e.kind {
                if matches!(&callee.kind,ExprKind::Name(p) if owned.contains(&p.segments.join(".")))
                {
                    errors.push(diagnostic(
                        "managed helpers cannot call back into an owned function",
                        e.span,
                    ));
                }
            }
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let span = original.span;
    managed_module.functions.push(Function {
        public: false,
        name: "main".into(),
        type_parameters: vec![],
        parameters: vec![],
        return_type: Some(TypeSyntax {
            path: Path {
                segments: vec!["i64".into()],
                span,
            },
            arguments: vec![],
            span,
        }),
        effects: None,
        capabilities: None,
        requires: vec![],
        ensures: vec![],
        foreign: None,
        body: vec![Stmt {
            kind: StmtKind::Expr(Expr {
                kind: ExprKind::Integer("0".into()),
                span,
            }),
            span,
        }],
        span,
    });
    // This is a compiler projection of the authenticated original source, not
    // that source's parsed AST. The outer envelope authenticates the projection.
    let managed = super::compile_ast(&managed_module)?;
    if managed.program().functions.iter().any(|f| {
        f.behavioral_effects
            .iter()
            .any(|i| managed.program().strings[*i] == "ReadArgs")
    }) {
        return Err(vec![diagnostic(
            "mixed owned/managed ReadArgs requires the later argument bridge gate",
            original.span,
        )]);
    }
    let mut projection = original.clone();
    projection.variants.clear();
    projection.records.retain(|r| {
        r.type_parameters.is_empty()
            && r.fields.iter().all(|f| {
                f.ty.arguments.is_empty()
                    && matches!(f.ty.path.segments.join(".").as_str(), "i64" | "Bool")
            })
    });
    let mut bridges = vec![];
    projection.functions.clear();
    let functions: BTreeMap<_, _> = original
        .functions
        .iter()
        .map(|f| (f.name.clone(), f))
        .collect();
    for f in &original.functions {
        if owned.contains(&f.name) {
            projection.functions.push(f.clone());
        } else if f.type_parameters.is_empty()
            && f.parameters.iter().all(|p| scalar(&p.ty))
            && f.return_type.as_ref().is_some_and(scalar)
        {
            let id = managed
                .program()
                .functions
                .iter()
                .position(|g| managed.program().strings[g.name] == f.name)
                .ok_or_else(|| vec![diagnostic("missing managed bridge specialization", f.span)])?;
            bridges.push((f.name.clone(), id));
            projection.functions.push(stub(f));
        }
    }
    let (owned_model, owned_digest) = ordinary_owned::analyze_projection(&projection, source).map_err(|mut errors| {
        errors.push(diagnostic("mixed owned bodies support copied-scalar managed helpers; direct managed allocation or affine capture requires a helper or later ownership gate", original.span));
        errors
    })?;
    // Bind and validate the real transitive collection summary across every
    // owned call and guard. The affine ghost body does not authorize collection.
    let mut collecting: BTreeMap<String, bool> =
        functions.keys().map(|n| (n.clone(), false)).collect();
    for (name, id) in &bridges {
        collecting.insert(
            name.clone(),
            managed.program().functions[*id]
                .effects
                .may_explicitly_collect,
        );
    }
    for _ in 0..=functions.len() {
        let old = collecting.clone();
        for name in &owned {
            let f = functions[name];
            let mut effect = false;
            cgx2_syntax::visit_stmts(&f.body, &mut |_| {}, &mut |e| {
                if let ExprKind::Call { callee, .. } = &e.kind {
                    if let ExprKind::Name(p) = &callee.kind {
                        effect |= old.get(&p.segments.join(".")).copied().unwrap_or(false);
                    }
                }
            });
            collecting.insert(name.clone(), effect);
        }
        if collecting == old {
            break;
        }
    }
    fn guarded(
        body: &[Stmt],
        active: bool,
        effects: &BTreeMap<String, bool>,
        errors: &mut Vec<TypeDiagnostic>,
    ) {
        for s in body {
            match &s.kind {
                StmtKind::NoGc { body, .. } => guarded(body, true, effects, errors),
                StmtKind::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    guarded(then_body, active, effects, errors);
                    guarded(else_body, active, effects, errors)
                }
                StmtKind::Match { arms, .. } => {
                    for a in arms {
                        guarded(&a.body, active, effects, errors)
                    }
                }
                _ => {}
            }
            if active {
                let mut calls = |e: &Expr| {
                    if let ExprKind::Call { callee, .. } = &e.kind {
                        if matches!(&callee.kind,ExprKind::Name(p) if effects.get(&p.segments.join(".")).copied().unwrap_or(false))
                        {
                            errors.push(diagnostic(
                                "explicit collection is forbidden transitively under no_gc",
                                e.span,
                            ));
                        }
                    }
                };
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::Assign { value, .. }
                    | StmtKind::Expr(value)
                    | StmtKind::Return(Some(value)) => cgx2_syntax::visit_expr(value, &mut calls),
                    StmtKind::If { condition, .. } => {
                        cgx2_syntax::visit_expr(condition, &mut calls)
                    }
                    StmtKind::Match { scrutinee, .. } => {
                        cgx2_syntax::visit_expr(scrutinee, &mut calls)
                    }
                    _ => {}
                }
            }
        }
    }
    for name in &owned {
        guarded(&functions[name].body, false, &collecting, &mut errors);
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let analysis_sha256 = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "hybrid-v1:{source}:{original:?}:{owned_digest}:{}:{bridges:?}",
                managed.analysis_sha256()
            )
            .as_bytes()
        )
    );
    Ok(HybridProgramV1 {
        source: source.into(),
        source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
        analysis_sha256,
        owned_model,
        managed,
        bridges,
    })
}
