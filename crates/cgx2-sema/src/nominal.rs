//! Nominal resolution never expands aggregate fields, so malformed cycles cannot
//! overflow the resolver. Declaration validation rejects recursive value shapes.
use crate::{ImportedSignatures, Type, TypeDiagnostic};
use cgx2_syntax::{Module, TypeSyntax};

pub(crate) fn identity(module: &Module, name: &str) -> String {
    if name.contains('.') {
        return name.to_owned();
    }
    module.name.as_ref().map_or_else(
        || name.to_owned(),
        |p| format!("{}.{name}", p.segments.join(".")),
    )
}

pub(crate) fn resolve(
    ty: &TypeSyntax,
    module: &Module,
    imported: &ImportedSignatures,
    diagnostics: &mut Vec<TypeDiagnostic>,
) -> Option<Type> {
    resolve_inner(ty, module, imported, diagnostics, true, 0)
}

pub(crate) fn resolve_member(
    ty: &TypeSyntax,
    module: &Module,
    imported: &ImportedSignatures,
    diagnostics: &mut Vec<TypeDiagnostic>,
) -> Option<Type> {
    resolve_inner(ty, module, imported, diagnostics, false, 0)
}

fn resolve_inner(
    ty: &TypeSyntax,
    module: &Module,
    imported: &ImportedSignatures,
    diagnostics: &mut Vec<TypeDiagnostic>,
    admission: bool,
    depth: usize,
) -> Option<Type> {
    let name = ty.path.segments.join(".");
    let fail = |diagnostics: &mut Vec<TypeDiagnostic>, code, message| {
        diagnostics.push(TypeDiagnostic::new(code, message, ty.span));
        None
    };
    if depth >= 64 {
        return fail(diagnostics, "CGT030", "type nesting exceeds 64".into());
    }
    let arity = match name.as_str() {
        "Option" => Some(1),
        "Result" => Some(2),
        "List" | "Array" => Some(1),
        _ => None,
    };
    if let Some(arity) = arity {
        if ty.arguments.len() != arity {
            return fail(
                diagnostics,
                if name == "List" { "CGT002" } else { "CGT031" },
                format!("type `{name}` requires {arity} arguments"),
            );
        }
        let arguments: Option<Vec<_>> = ty
            .arguments
            .iter()
            .map(|t| resolve_inner(t, module, imported, diagnostics, admission, depth + 1))
            .collect();
        let arguments = arguments?;
        fn has_list(ty: &Type) -> bool {
            match ty {
                Type::List(_) => true,
                Type::Variant { arguments, .. } => arguments.iter().any(has_list),
                _ => false,
            }
        }
        if matches!(name.as_str(), "Option" | "Result") && arguments.iter().any(has_list) {
            return fail(
                diagnostics,
                "CGT032",
                "collection payload semantics are deferred to S2b".into(),
            );
        }
        return match name.as_str() {
            "List" if matches!(arguments[0], Type::Record(_)) => fail(
                diagnostics,
                "CGT023",
                "record values in generic containers are unsupported".into(),
            ),
            "Option" | "Result" => Some(Type::Variant {
                name: format!("cgx2.core.{name}"),
                arguments,
            }),
            "List" if !matches!(arguments[0], Type::Record(_) | Type::Variant { .. }) => {
                Some(Type::List(Box::new(arguments[0].clone())))
            }
            _ => fail(
                diagnostics,
                "CGT032",
                format!("{name} aggregate/collection semantics are deferred to S2b"),
            ),
        };
    }
    if !ty.arguments.is_empty() {
        return fail(
            diagnostics,
            "CGT031",
            format!("type `{name}` accepts no arguments in S2a"),
        );
    }
    match name.as_str() {
        "i64" => return Some(Type::I64),
        "Bool" => return Some(Type::Bool),
        "String" => return Some(Type::String),
        "Unit" => return Some(Type::Unit),
        _ => {}
    }
    let own = identity(module, &name);
    if module
        .records
        .iter()
        .any(|r| identity(module, &r.name) == own)
    {
        return Some(Type::Record(own));
    }
    if module
        .variants
        .iter()
        .any(|v| identity(module, &v.name) == own)
    {
        return Some(Type::Variant {
            name: own,
            arguments: vec![],
        });
    }
    if imported.private_records.contains(&name) || imported.private_variants.contains(&name) {
        return fail(diagnostics, "CGT022", format!("type `{name}` is private"));
    }
    if admission && ty.path.segments.len() > 1 {
        let prefix = ty.path.segments[..ty.path.segments.len() - 1].join(".");
        if !module
            .imports
            .iter()
            .any(|p| p.segments.join(".") == prefix)
        {
            return fail(
                diagnostics,
                "CGT008",
                format!("module `{prefix}` is not imported"),
            );
        }
    }
    if imported.public_records.contains_key(&name) {
        return Some(Type::Record(name));
    }
    if imported.public_variants.contains_key(&name) {
        return Some(Type::Variant {
            name,
            arguments: vec![],
        });
    }
    fail(
        diagnostics,
        "CGT002",
        format!("unknown or unsupported type `{ty}`"),
    )
}

pub(crate) fn cases(
    ty: &Type,
    module: &Module,
    imported: &ImportedSignatures,
    diagnostics: &mut Vec<TypeDiagnostic>,
) -> Option<Vec<(String, Vec<Type>)>> {
    let Type::Variant { name, arguments } = ty else {
        return None;
    };
    if name == "cgx2.core.Option" && arguments.len() == 1 {
        return Some(vec![
            ("Some".into(), vec![arguments[0].clone()]),
            ("None".into(), vec![]),
        ]);
    }
    if name == "cgx2.core.Result" && arguments.len() == 2 {
        return Some(vec![
            ("Ok".into(), vec![arguments[0].clone()]),
            ("Err".into(), vec![arguments[1].clone()]),
        ]);
    }
    let variant = module
        .variants
        .iter()
        .find(|v| identity(module, &v.name) == *name)
        .or_else(|| imported.public_variants.get(name))?;
    variant
        .cases
        .iter()
        .map(|case| {
            let payloads: Option<Vec<_>> = case
                .payloads
                .iter()
                .map(|t| resolve_member(t, module, imported, diagnostics))
                .collect();
            Some((case.name.clone(), payloads?))
        })
        .collect()
}

pub(crate) fn private_type(ty: &Type, module: &Module) -> bool {
    match ty {
        Type::Record(name) => module
            .records
            .iter()
            .any(|r| !r.public && identity(module, &r.name) == *name),
        Type::Variant { name, arguments } => {
            module
                .variants
                .iter()
                .any(|v| !v.public && identity(module, &v.name) == *name)
                || arguments.iter().any(|t| private_type(t, module))
        }
        Type::List(inner) => private_type(inner, module),
        _ => false,
    }
}
