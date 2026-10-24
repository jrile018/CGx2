//! Conservative consumer admission marker. Source execution resolves and
//! authenticates compiler identities in the ordinary-memory producer.
use crate::{visit_stmts, ExprKind, Module, Span};

fn identity(module: &Module, name: &str) -> String {
    if name.contains('.') {
        return name.into();
    }
    module.name.as_ref().map_or_else(
        || name.into(),
        |m| format!("{}.{}", m.segments.join("."), name),
    )
}
/// Declaration resolution for the compiler memory namespace. This describes
/// names, never a caller assertion that a value is a trusted capability.
pub fn ordinary_memory_builtin(module: &Module, name: &str) -> bool {
    if !matches!(
        name,
        "memory.try_region"
            | "memory.try_in"
            | "memory.copy_to"
            | "memory.promote"
            | "memory.copy_owned"
            | "memory.default_region"
            | "memory.collect"
            | "memory.try_no_gc"
            | "memory.try_session"
    ) {
        return false;
    }
    if module
        .functions
        .iter()
        .any(|f| f.name == name || identity(module, &f.name) == name)
    {
        return false;
    }
    let nominal = |n: &str| {
        let qualified = identity(module, n);
        n == name
            || qualified == name
            || name.starts_with(&format!("{n}."))
            || name.starts_with(&format!("{qualified}."))
    };
    !module.records.iter().any(|r| nominal(&r.name))
        && !module.variants.iter().any(|v| nominal(&v.name))
}
pub fn ordinary_memory_span(module: &Module) -> Option<Span> {
    let found = std::cell::Cell::new(None);
    for body in std::iter::once(&module.statements).chain(module.functions.iter().map(|f| &f.body))
    {
        visit_stmts(body, &mut |_| {}, &mut |e| {
            if let ExprKind::Call { callee, .. } = &e.kind {
                if let ExprKind::Name(p) = &callee.kind {
                    let name = p.segments.join(".");
                    if name != "memory.collect"
                        && ordinary_memory_builtin(module, &name)
                        && found.get().is_none()
                    {
                        found.set(Some(e.span));
                    }
                }
            }
        });
    }
    found.get()
}
