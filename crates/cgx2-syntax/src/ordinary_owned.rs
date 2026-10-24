//! Admission marker only; resolution and ownership checking live in sema.
use crate::{ExprKind, Module, Span, StmtKind, TypeSyntax};

pub fn ordinary_owned_span(module: &Module) -> Option<Span> {
    fn wrapper(ty: &TypeSyntax) -> Option<Span> {
        if !ty.arguments.is_empty()
            && matches!(
                ty.path.segments.join(".").as_str(),
                "owned" | "shared" | "exclusive"
            )
        {
            Some(ty.span)
        } else {
            ty.arguments.iter().find_map(wrapper)
        }
    }
    for f in &module.functions {
        if let Some(s) = f
            .parameters
            .iter()
            .find_map(|p| wrapper(&p.ty))
            .or_else(|| f.return_type.as_ref().and_then(wrapper))
        {
            return Some(s);
        }
    }
    for r in &module.records {
        if let Some(s) = r.fields.iter().find_map(|f| wrapper(&f.ty)) {
            return Some(s);
        }
    }
    for v in &module.variants {
        if let Some(s) = v.cases.iter().flat_map(|c| &c.payloads).find_map(wrapper) {
            return Some(s);
        }
    }
    let found = std::cell::Cell::new(None);
    let guard = std::cell::Cell::new(None);
    // The pre-existing memory-function profile requires explicit Unit/effects.
    // Keep even guard-only legacy functions on their historical budget path.
    let legacy_memory = std::cell::Cell::new(
        !module.functions.is_empty()
            && module.functions.iter().all(|f| {
                f.effects.is_some()
                    && f.return_type
                        .as_ref()
                        .is_some_and(|t| t.arguments.is_empty() && t.path.segments == ["Unit"])
            }),
    );
    let inspect = |e: &crate::Expr| {
        if let ExprKind::Call { callee, .. } = &e.kind {
            if let ExprKind::Name(p) = &callee.kind {
                let name = p.segments.join(".");
                if name.starts_with("memory.") {
                    legacy_memory.set(true);
                }
                if matches!(
                    name.as_str(),
                    "owned" | "try_owned" | "borrow" | "borrow_mut"
                ) && !module.functions.iter().any(|f| f.name == name)
                    && !module.records.iter().any(|r| r.name == name)
                {
                    found.set(Some(e.span));
                }
            }
        }
    };
    for body in std::iter::once(&module.statements).chain(module.functions.iter().map(|f| &f.body))
    {
        crate::visit_stmts(
            body,
            &mut |s| {
                if matches!(s.kind, StmtKind::NoGc { .. }) && module.statements.is_empty() {
                    guard.set(Some(s.span));
                }
                if let StmtKind::Let {
                    annotation: Some(t),
                    ..
                } = &s.kind
                {
                    if let Some(span) = wrapper(t) {
                        found.set(Some(span));
                    }
                }
            },
            &mut |e| inspect(e),
        );
    }
    found.get().or_else(|| {
        if legacy_memory.get() {
            None
        } else {
            guard.get()
        }
    })
}
