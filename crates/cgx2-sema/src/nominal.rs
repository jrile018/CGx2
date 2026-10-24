//! Bounded nominal resolution with declaration-scoped binders.
use crate::{ImportedSignatures, Type, TypeDiagnostic};
use cgx2_syntax::{Module, TypeParameter, TypeSyntax};
use std::collections::BTreeMap;

pub(crate) fn identity(module: &Module, name: &str) -> String {
    if name.contains('.') {
        return name.to_owned();
    }
    module.name.as_ref().map_or_else(
        || name.to_owned(),
        |p| format!("{}.{name}", p.segments.join(".")),
    )
}
pub(crate) fn binders(
    module: &Module,
    owner: &str,
    params: &[TypeParameter],
    errors: &mut Vec<TypeDiagnostic>,
) -> BTreeMap<String, Type> {
    let mut result = BTreeMap::new();
    for p in params {
        let mut constraints = Vec::new();
        for c in &p.constraints {
            let name = c.segments.join(".");
            if !matches!(name.as_str(), "Eq" | "Show") || constraints.contains(&name) {
                errors.push(TypeDiagnostic::new(
                    "CGT039",
                    format!(
                        "unknown or ambiguous constraint `{name}`; supported constraints: Eq, Show"
                    ),
                    c.span,
                ));
            }
            constraints.push(name);
        }
        constraints.sort();
        if result
            .insert(
                p.name.clone(),
                Type::Parameter {
                    name: format!("{}::{}", identity(module, owner), p.name),
                    constraints,
                },
            )
            .is_some()
        {
            errors.push(TypeDiagnostic::new(
                "CGT031",
                format!("duplicate type parameter `{}`", p.name),
                p.span,
            ));
        }
        if matches!(
            p.name.as_str(),
            "i64" | "Bool" | "String" | "Unit" | "List" | "Array" | "Option" | "Result"
        ) {
            errors.push(TypeDiagnostic::new(
                "CGT031",
                "type parameter shadows a built-in type".into(),
                p.span,
            ));
        }
    }
    result
}
pub(crate) fn resolve(
    ty: &TypeSyntax,
    module: &Module,
    imported: &ImportedSignatures,
    errors: &mut Vec<TypeDiagnostic>,
) -> Option<Type> {
    scoped(ty, module, imported, &BTreeMap::new(), errors)
}
pub(crate) fn resolve_member(
    ty: &TypeSyntax,
    module: &Module,
    imported: &ImportedSignatures,
    errors: &mut Vec<TypeDiagnostic>,
) -> Option<Type> {
    inner(ty, module, imported, &BTreeMap::new(), errors, false, 0)
}
pub(crate) fn scoped(
    ty: &TypeSyntax,
    module: &Module,
    imported: &ImportedSignatures,
    scope: &BTreeMap<String, Type>,
    errors: &mut Vec<TypeDiagnostic>,
) -> Option<Type> {
    inner(ty, module, imported, scope, errors, true, 0)
}
fn inner(
    ty: &TypeSyntax,
    module: &Module,
    imported: &ImportedSignatures,
    scope: &BTreeMap<String, Type>,
    errors: &mut Vec<TypeDiagnostic>,
    admission: bool,
    depth: usize,
) -> Option<Type> {
    let name = ty.path.segments.join(".");
    let fail = |errors: &mut Vec<TypeDiagnostic>, code, message| {
        errors.push(TypeDiagnostic::new(code, message, ty.span));
        None
    };
    if depth >= 64 {
        return fail(errors, "CGT030", "type depth exceeds 64".into());
    }
    if let Some(param) = scope.get(&name) {
        return if ty.arguments.is_empty() {
            Some(param.clone())
        } else {
            fail(
                errors,
                "CGT031",
                "type parameter accepts no arguments".into(),
            )
        };
    }
    let own = identity(module, &name);
    let record = module
        .records
        .iter()
        .find(|r| identity(module, &r.name) == own)
        .or_else(|| imported.public_records.get(&name));
    let variant = module
        .variants
        .iter()
        .find(|v| identity(module, &v.name) == own)
        .or_else(|| imported.public_variants.get(&name));
    let builtin = match name.as_str() {
        "Option" | "cgx2.core.Option" | "List" | "Array" => Some(1),
        "Result" | "cgx2.core.Result" => Some(2),
        "i64" | "Bool" | "String" | "Unit" => Some(0),
        _ => None,
    };
    if builtin.is_none() && record.is_none() && variant.is_none() {
        return fail(
            errors,
            if imported.private_records.contains(&name) || imported.private_variants.contains(&name)
            {
                "CGT022"
            } else {
                "CGT002"
            },
            format!("unknown, unavailable or private type `{name}`"),
        );
    }
    if builtin.is_none()
        && admission
        && !module
            .records
            .iter()
            .any(|r| identity(module, &r.name) == own)
        && !module
            .variants
            .iter()
            .any(|v| identity(module, &v.name) == own)
        && ty.path.segments.len() > 1
    {
        let prefix = ty.path.segments[..ty.path.segments.len() - 1].join(".");
        if !module
            .imports
            .iter()
            .any(|p| p.segments.join(".") == prefix)
        {
            return fail(
                errors,
                "CGT008",
                format!("type `{name}` requires direct import of `{prefix}`"),
            );
        }
    }
    let parameters = record
        .map(|r| r.type_parameters.as_slice())
        .or_else(|| variant.map(|v| v.type_parameters.as_slice()))
        .unwrap_or(&[]);
    let arity = builtin.unwrap_or(parameters.len());
    if ty.arguments.len() != arity {
        return fail(
            errors,
            if name == "List" { "CGT002" } else { "CGT031" },
            format!(
                "type `{name}` requires {arity} type arguments, found {}",
                ty.arguments.len()
            ),
        );
    }
    let args = ty
        .arguments
        .iter()
        .map(|t| inner(t, module, imported, scope, errors, admission, depth + 1))
        .collect::<Option<Vec<_>>>()?;
    for (p, arg) in parameters.iter().zip(&args).filter(|_| admission) {
        for c in &p.constraints {
            if !satisfies(arg, &c.segments.join("."), module, imported, 0) {
                errors.push(TypeDiagnostic::new(
                    "CGT039",
                    format!("type `{arg}` does not satisfy `{}`", c.segments.join(".")),
                    ty.span,
                ));
            }
        }
    }
    Some(match name.as_str() {
        "i64" => Type::I64,
        "Bool" => Type::Bool,
        "String" => Type::String,
        "Unit" => Type::Unit,
        "List" => Type::List(Box::new(args[0].clone())),
        "Array" => Type::Array(Box::new(args[0].clone())),
        "Option" | "cgx2.core.Option" => Type::Variant {
            name: "cgx2.core.Option".into(),
            arguments: args,
        },
        "Result" | "cgx2.core.Result" => Type::Variant {
            name: "cgx2.core.Result".into(),
            arguments: args,
        },
        _ if record.is_some() => {
            let n = record.unwrap().name.clone();
            let n = identity(module, &n);
            if args.is_empty() {
                Type::Record(n)
            } else {
                Type::AppliedRecord {
                    name: n,
                    arguments: args,
                }
            }
        }
        _ => Type::Variant {
            name: identity(module, &variant.unwrap().name),
            arguments: args,
        },
    })
}
pub(crate) fn substitute(ty: &Type, map: &BTreeMap<String, Type>) -> Type {
    match ty {
        Type::Parameter { name, .. } => map.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(t) => Type::List(Box::new(substitute(t, map))),
        Type::Array(t) => Type::Array(Box::new(substitute(t, map))),
        Type::Variant { name, arguments } => Type::Variant {
            name: name.clone(),
            arguments: arguments.iter().map(|t| substitute(t, map)).collect(),
        },
        Type::AppliedRecord { name, arguments } => Type::AppliedRecord {
            name: name.clone(),
            arguments: arguments.iter().map(|t| substitute(t, map)).collect(),
        },
        _ => ty.clone(),
    }
}
pub(crate) fn members(
    ty: &Type,
    module: &Module,
    imported: &ImportedSignatures,
    errors: &mut Vec<TypeDiagnostic>,
) -> Option<Vec<(String, Type)>> {
    let (name, args) = match ty {
        Type::Record(n) => (n, &[][..]),
        Type::AppliedRecord { name, arguments } => (name, arguments.as_slice()),
        _ => return None,
    };
    let r = module
        .records
        .iter()
        .find(|r| identity(module, &r.name) == *name)
        .or_else(|| imported.public_records.get(name))?;
    let scope = r
        .type_parameters
        .iter()
        .zip(args)
        .map(|(p, t)| (p.name.clone(), t.clone()))
        .collect();
    r.fields
        .iter()
        .map(|f| {
            Some((
                f.name.clone(),
                inner(&f.ty, module, imported, &scope, errors, false, 0)?,
            ))
        })
        .collect()
}
pub(crate) fn cases(
    ty: &Type,
    module: &Module,
    imported: &ImportedSignatures,
    errors: &mut Vec<TypeDiagnostic>,
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
    let v = module
        .variants
        .iter()
        .find(|v| identity(module, &v.name) == *name)
        .or_else(|| imported.public_variants.get(name))?;
    let scope = v
        .type_parameters
        .iter()
        .zip(arguments)
        .map(|(p, t)| (p.name.clone(), t.clone()))
        .collect();
    v.cases
        .iter()
        .map(|c| {
            Some((
                c.name.clone(),
                c.payloads
                    .iter()
                    .map(|t| inner(t, module, imported, &scope, errors, false, 0))
                    .collect::<Option<Vec<_>>>()?,
            ))
        })
        .collect()
}
pub(crate) fn satisfies(
    ty: &Type,
    constraint: &str,
    module: &Module,
    imported: &ImportedSignatures,
    depth: usize,
) -> bool {
    fn walk(
        ty: &Type,
        c: &str,
        module: &Module,
        imported: &ImportedSignatures,
        depth: usize,
        memo: &mut BTreeMap<String, bool>,
    ) -> bool {
        if depth >= 64 || memo.len() >= 4096 || !matches!(c, "Eq" | "Show") {
            return false;
        }
        let key = crate::type_key(ty);
        if let Some(result) = memo.get(&key) {
            return *result;
        }
        // Mark in-progress false: malformed recursive declarations cannot recurse
        // through constraint checking before aggregate validation diagnoses them.
        memo.insert(key.clone(), false);
        let result = match ty {
            Type::Parameter { constraints, .. } => constraints.iter().any(|n| n == c),
            Type::List(t) | Type::Array(t) => walk(t, c, module, imported, depth + 1, memo),
            Type::Record(_) | Type::AppliedRecord { .. } => {
                members(ty, module, imported, &mut vec![]).is_some_and(|fields| {
                    fields
                        .iter()
                        .all(|(_, t)| walk(t, c, module, imported, depth + 1, memo))
                })
            }
            Type::Variant { .. } => cases(ty, module, imported, &mut vec![]).is_some_and(|cases| {
                cases.iter().all(|(_, ts)| {
                    ts.iter()
                        .all(|t| walk(t, c, module, imported, depth + 1, memo))
                })
            }),
            _ => true,
        };
        memo.insert(key, result);
        result
    }
    walk(
        ty,
        constraint,
        module,
        imported,
        depth,
        &mut BTreeMap::new(),
    )
}
pub(crate) fn private_type(ty: &Type, module: &Module) -> bool {
    match ty {
        Type::Record(n) => module
            .records
            .iter()
            .any(|r| !r.public && identity(module, &r.name) == *n),
        Type::AppliedRecord { name, arguments } => {
            module
                .records
                .iter()
                .any(|r| !r.public && identity(module, &r.name) == *name)
                || arguments.iter().any(|t| private_type(t, module))
        }
        Type::Variant { name, arguments } => {
            module
                .variants
                .iter()
                .any(|v| !v.public && identity(module, &v.name) == *name)
                || arguments.iter().any(|t| private_type(t, module))
        }
        Type::List(t) | Type::Array(t) => private_type(t, module),
        _ => false,
    }
}

pub(crate) fn try_substitute(ty: &Type, map: &BTreeMap<String, Type>) -> Option<Type> {
    fn walk(
        t: &Type,
        map: &BTreeMap<String, Type>,
        left: &mut usize,
        depth: usize,
    ) -> Option<Type> {
        if *left == 0 || depth >= 64 {
            return None;
        }
        *left -= 1;
        Some(match t {
            Type::Parameter { name, .. } if map.contains_key(name) => {
                walk(&map[name], &BTreeMap::new(), left, depth + 1)?
            }
            Type::List(t) => Type::List(Box::new(walk(t, map, left, depth + 1)?)),
            Type::Array(t) => Type::Array(Box::new(walk(t, map, left, depth + 1)?)),
            Type::AppliedRecord { name, arguments } => Type::AppliedRecord {
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|t| walk(t, map, left, depth + 1))
                    .collect::<Option<Vec<_>>>()?,
            },
            Type::Variant { name, arguments } => Type::Variant {
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|t| walk(t, map, left, depth + 1))
                    .collect::<Option<Vec<_>>>()?,
            },
            _ => t.clone(),
        })
    }
    walk(ty, map, &mut 4096, 0)
}
