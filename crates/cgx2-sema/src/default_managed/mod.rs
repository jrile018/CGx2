//! Sole source producer for ordinary managed graphs, independent of affine v1/v2.
pub mod hybrid;
mod lower;
use crate::{Type, TypeDiagnostic, TypedModule};
pub use cgx2_runtime::default_managed::model;
use cgx2_runtime::default_managed::{model::Program, Execution};
use cgx2_syntax::{Module, Span};
use sha2::{Digest, Sha256};

/// New syntax marker only. Ordinary immutable applications also use managed storage.
pub fn explicit(module: &Module) -> bool {
    if cgx2_syntax::managed_feature_span(module).is_some() {
        return true;
    }
    if guard_span(module).is_none() || module.functions.iter().any(|f| hybrid::affine(module, f)) {
        return false;
    }
    let legacy = std::cell::Cell::new(false);
    let heap = std::cell::Cell::new(
        !module.records.is_empty()
            || !module.variants.is_empty()
            || crate::foundation_profile(module),
    );
    let mut expr = |e: &cgx2_syntax::Expr| match &e.kind {
        cgx2_syntax::ExprKind::StringLiteral(_) => heap.set(true),
        cgx2_syntax::ExprKind::Call { callee, .. } => {
            if let cgx2_syntax::ExprKind::Name(p) = &callee.kind {
                if p.segments.first().is_some_and(|n| n == "memory") {
                    legacy.set(true);
                }
                if p.segments == ["arg"] {
                    heap.set(true);
                }
            }
        }
        _ => {}
    };
    cgx2_syntax::visit_stmts(&module.statements, &mut |_| {}, &mut expr);
    for f in &module.functions {
        cgx2_syntax::visit_stmts(&f.body, &mut |_| {}, &mut expr);
    }
    !legacy.get() && (heap.get() || !module.imports.is_empty())
}
pub fn guard_span(module: &Module) -> Option<Span> {
    let found = std::cell::Cell::new(None);
    for body in std::iter::once(&module.statements).chain(module.functions.iter().map(|f| &f.body))
    {
        cgx2_syntax::visit_stmts(
            body,
            &mut |s| {
                if matches!(s.kind, cgx2_syntax::StmtKind::NoGc { .. }) {
                    found.set(Some(s.span));
                }
            },
            &mut |_| {},
        );
    }
    found.get()
}
/// Target admission uses resolved calls, so a user package named `managed`
/// retains its functions and nominal constructors rather than becoming a cell.
pub fn resolved_explicit(module: &Module, typed: &TypedModule) -> bool {
    if profile(module) && guard_span(module).is_some() {
        return true;
    }
    fn cell(t: &cgx2_syntax::TypeSyntax) -> bool {
        (matches!(
            t.path.segments.join(".").as_str(),
            "managed.Ref" | "cgx2.managed.Ref"
        ) && t.arguments.len() == 1)
            || t.arguments.iter().any(cell)
    }
    if module
        .records
        .iter()
        .any(|r| r.fields.iter().any(|f| cell(&f.ty)))
        || module
            .variants
            .iter()
            .any(|v| v.cases.iter().any(|c| c.payloads.iter().any(cell)))
        || module.functions.iter().any(|f| {
            f.parameters.iter().any(|p| cell(&p.ty)) || f.return_type.as_ref().is_some_and(cell)
        })
    {
        return true;
    }
    let found = std::cell::Cell::new(false);
    for (owner, body) in std::iter::once(("", &module.statements))
        .chain(module.functions.iter().map(|f| (f.name.as_str(), &f.body)))
    {
        cgx2_syntax::visit_stmts(
            body,
            &mut |s| {
                if matches!(&s.kind,cgx2_syntax::StmtKind::Let{annotation:Some(t),..} if cell(t)) {
                    found.set(true);
                }
            },
            &mut |e| {
                if let cgx2_syntax::ExprKind::Call { callee, .. } = &e.kind {
                    if let cgx2_syntax::ExprKind::Name(p) = &callee.kind {
                        let name = p.segments.join(".");
                        let user=typed.calls.contains_key(&(owner.to_owned(),e.span.start,e.span.end))||typed.functions.contains_key(&name)||typed.bodies.get(owner).and_then(|b|b.get(e.span)).is_some_and(|t|matches!(t,Type::Record(n)|Type::AppliedRecord{name:n,..} if *n==name));
                        if !user
                            && matches!(
                                name.as_str(),
                                "managed.ref"
                                    | "managed.get"
                                    | "managed.set"
                                    | "managed.collect"
                                    | "managed.try_alloc"
                            )
                        {
                            found.set(true);
                        }
                    }
                }
            },
        );
    }
    found.get()
}
/// Ordinary immutable heap applications. Legacy memory and affine profiles keep
/// their reviewed source paths unless an explicit managed operation selects this one.
pub fn profile(module: &Module) -> bool {
    if explicit(module) {
        return true;
    }
    if cgx2_syntax::ordinary_owned_span(module).is_some()
        || module.functions.iter().any(|f| f.foreign.is_some())
    {
        return false;
    }
    let legacy = std::cell::Cell::new(false);
    let string = std::cell::Cell::new(false);
    let args = std::cell::Cell::new(false);
    let mut inspect = |e: &cgx2_syntax::Expr| match &e.kind {
        cgx2_syntax::ExprKind::StringLiteral(_) => string.set(true),
        cgx2_syntax::ExprKind::Call { callee, .. } => {
            if matches!(&callee.kind,cgx2_syntax::ExprKind::Name(p) if p.segments==["arg"]) {
                args.set(true);
            }
            if matches!(&callee.kind,cgx2_syntax::ExprKind::Name(p) if p.segments.first().is_some_and(|n|n=="memory"))
            {
                legacy.set(true);
            }
        }
        _ => {}
    };
    cgx2_syntax::visit_stmts(&module.statements, &mut |_| {}, &mut inspect);
    for f in &module.functions {
        cgx2_syntax::visit_stmts(&f.body, &mut |_| {}, &mut inspect);
    }
    !legacy.get()
        && (string.get()
            // Preserve the legacy target's diagnostics for malformed scalar
            // argument calls. Valid String-producing calls use managed storage.
            || (args.get() && crate::check_module(module).is_ok())
            || !module.records.is_empty()
            || !module.variants.is_empty()
            || crate::foundation_profile(module)
            || cgx2_syntax::s2a_feature_span(module).is_some())
}
/// Private immutable fields: execution/emission always reconstruct and compare the producer.
#[derive(Clone, Debug)]
pub struct ManagedProgramV1 {
    module: Module,
    typed: TypedModule,
    program: Program,
    types: Vec<Type>,
    source: String,
    source_sha256: String,
    digest: String,
}
impl ManagedProgramV1 {
    pub fn program(&self) -> &Program {
        &self.program
    }
    pub fn types(&self) -> &[Type] {
        &self.types
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn analysis_sha256(&self) -> &str {
        &self.digest
    }
    pub fn authenticate(&self) -> Result<(), Vec<TypeDiagnostic>> {
        let fresh = compile_module(&self.module, &self.source)?;
        if self.source_sha256 != fresh.source_sha256
            || self.digest != fresh.digest
            || self.program != fresh.program
            || self.types != fresh.types
            || self.typed != fresh.typed
        {
            return Err(vec![diagnostic(
                "stale or forged managed program",
                self.module.span,
            )]);
        }
        Ok(())
    }
    pub fn execute(&self, args: &[String]) -> Result<Execution, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(cgx2_runtime::default_managed::run(&self.program, args))
    }
    pub fn execute_with_session(
        &self,
        args: &[String],
        session: cgx2_runtime::Session,
    ) -> Result<Execution, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        Ok(cgx2_runtime::default_managed::run_with_session(
            &self.program,
            args,
            session,
        ))
    }
    pub fn rust_data(&self) -> Result<String, Vec<TypeDiagnostic>> {
        self.authenticate()?;
        if self.program.expressions.len() > 2048 {
            let restored = Program::decode_wire_v1(&self.program.wire_v1())
                .map_err(|message| vec![diagnostic(message, self.module.span)])?;
            if restored != self.program {
                return Err(vec![diagnostic(
                    "managed model encoding mismatch",
                    self.module.span,
                )]);
            }
        }
        Ok(self.program.rust())
    }
}
pub fn compile_source(source: &str) -> Result<ManagedProgramV1, Vec<TypeDiagnostic>> {
    let module = cgx2_syntax::parse_module(source).map_err(|ds| {
        ds.into_iter()
            .map(|d| TypeDiagnostic {
                code: d.code,
                message: d.message,
                span: d.span,
            })
            .collect::<Vec<_>>()
    })?;
    compile_module(&module, source)
}
/// Package callers must verify their CheckedPackage immediately before producing
/// this merged source; original visibility/effect authentication remains mandatory.
pub fn compile_module(
    module: &Module,
    source: &str,
) -> Result<ManagedProgramV1, Vec<TypeDiagnostic>> {
    if source.len() > 4 * 1024 * 1024 {
        return Err(vec![diagnostic(
            "managed source exceeds 4 MiB",
            module.span,
        )]);
    }
    let typed = crate::check_module(module)?;
    let concrete = crate::lower_module(module, &typed)?;
    let checked = crate::check_module_with_imports(&concrete, &Default::default())?;
    let (program, types) = lower::lower(&concrete, &checked)?;
    let mut hash = Sha256::new();
    hash.update(b"CGX2-MANAGED-PROGRAM-V1\0");
    hash.update(source.as_bytes());
    hash.update(format!("{module:?}{typed:?}{program:?}").as_bytes());
    Ok(ManagedProgramV1 {
        module: module.clone(),
        typed,
        program,
        types,
        source: source.into(),
        source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
        digest: format!("{:x}", hash.finalize()),
    })
}
fn diagnostic(message: impl Into<String>, span: Span) -> TypeDiagnostic {
    TypeDiagnostic {
        code: "CGT090",
        message: message.into(),
        span,
    }
}
