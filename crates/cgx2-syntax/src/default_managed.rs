//! Candidate admission marker; checked resolution remains in sema.
use crate::{ExprKind, Module, Span, StmtKind, TypeSyntax};
pub fn managed_feature_span(module: &Module) -> Option<Span> {
    fn ty(t: &TypeSyntax) -> Option<Span> {
        if matches!(
            t.path.segments.join(".").as_str(),
            "managed.Ref" | "cgx2.managed.Ref"
        ) {
            Some(t.span)
        } else {
            t.arguments.iter().find_map(ty)
        }
    }
    for f in &module.functions {
        if let Some(span) = f
            .parameters
            .iter()
            .find_map(|p| ty(&p.ty))
            .or_else(|| f.return_type.as_ref().and_then(ty))
        {
            return Some(span);
        }
    }
    for r in &module.records {
        if let Some(span) = r.fields.iter().find_map(|f| ty(&f.ty)) {
            return Some(span);
        }
    }
    for v in &module.variants {
        if let Some(span) = v.cases.iter().flat_map(|c| &c.payloads).find_map(ty) {
            return Some(span);
        }
    }
    let found = std::cell::Cell::new(None);
    for body in std::iter::once(&module.statements).chain(module.functions.iter().map(|f| &f.body))
    {
        crate::visit_stmts(
            body,
            &mut |s| {
                if let StmtKind::Let {
                    annotation: Some(t),
                    ..
                } = &s.kind
                {
                    if let Some(span) = ty(t) {
                        found.set(Some(span));
                    }
                }
            },
            &mut |e| {
                if let ExprKind::Call { callee, .. } = &e.kind {
                    if matches!(&callee.kind,ExprKind::Name(p) if matches!(p.segments.join(".").as_str(),"managed.ref"|"managed.get"|"managed.set"|"managed.collect"|"managed.try_alloc"))
                    {
                        found.set(Some(e.span));
                    }
                }
            },
        );
    }
    found.get()
}

/// First syntactic no_gc boundary, for consumers that conservatively gate guards.
pub fn managed_guard_span(module: &Module) -> Option<Span> {
    let found = std::cell::Cell::new(None);
    for body in std::iter::once(&module.statements).chain(module.functions.iter().map(|f| &f.body))
    {
        crate::visit_stmts(
            body,
            &mut |s| {
                if matches!(s.kind, crate::StmtKind::NoGc { .. }) && found.get().is_none() {
                    found.set(Some(s.span));
                }
            },
            &mut |_| {},
        );
    }
    found.get()
}
