//! Baseline static semantics for the Wave 1 CGx2 syntax tree.

mod check;
mod diagnostic;
mod effects;
mod imports;
mod memory;
pub use memory::graph_memory_profile;
pub mod ordinary_owned;
mod tensor;
mod types;

pub use check::{
    check_module, check_module_with_imports, collect_public_signatures,
    collect_public_signatures_with_imports, FunctionSignature, ImportedSignatures, ResolvedCall,
    SpanTypes, TypedModule,
};
pub use diagnostic::TypeDiagnostic;
pub use effects::EffectSummary;
pub use imports::CheckedImportedEffects;
pub use tensor::{tensor_profile, MatrixLiteral, MatrixLiteralF32, TensorOperation, TensorProfile};
pub use types::Type;

mod foundation;
mod nominal;
pub use foundation::{lower_module, merge_modules};

/// Resolve a source type without expanding aggregate payloads. Used by checked
/// consumers to retain nominal identity rather than guessing runtime string tags.
pub fn resolve_type(
    module: &cgx2_syntax::Module,
    imports: &ImportedSignatures,
    ty: &cgx2_syntax::TypeSyntax,
) -> Result<Type, Vec<TypeDiagnostic>> {
    let mut diagnostics = Vec::new();
    let result = nominal::resolve(ty, module, imports, &mut diagnostics);
    if diagnostics.is_empty() {
        result.ok_or(diagnostics)
    } else {
        Err(diagnostics)
    }
}

pub fn variant_cases(
    module: &cgx2_syntax::Module,
    imports: &ImportedSignatures,
    ty: &Type,
) -> Option<Vec<(String, Vec<Type>)>> {
    nominal::cases(ty, module, imports, &mut Vec::new())
}

/// Finite generic/data profile requiring S2b lowering. Older consumers must
/// continue to use their conservative S2a admission gate.
pub fn foundation_profile(module: &cgx2_syntax::Module) -> bool {
    fn data_type(t: &cgx2_syntax::TypeSyntax) -> bool {
        matches!(t.path.segments.join(".").as_str(), "List" | "Array")
            || t.arguments.iter().any(data_type)
    }
    module
        .records
        .iter()
        .any(|r| r.fields.iter().any(|f| data_type(&f.ty)))
        || module
            .variants
            .iter()
            .any(|v| v.cases.iter().any(|c| c.payloads.iter().any(data_type)))
        || module.functions.iter().any(|f| {
            f.parameters.iter().any(|p| data_type(&p.ty))
                || f.return_type.as_ref().is_some_and(data_type)
        })
        || module
            .functions
            .iter()
            .any(|f| !f.type_parameters.is_empty())
        || module.records.iter().any(|r| !r.type_parameters.is_empty())
        || module
            .variants
            .iter()
            .any(|v| !v.type_parameters.is_empty())
        || {
            let found = std::cell::Cell::new(false);
            let mut visitor = |e: &cgx2_syntax::Expr| match &e.kind {
                cgx2_syntax::ExprKind::Collection(_) | cgx2_syntax::ExprKind::Index { .. } => {
                    found.set(true)
                }
                cgx2_syntax::ExprKind::Constructor { case, .. } if case.is_empty() => {
                    found.set(true)
                }
                cgx2_syntax::ExprKind::Call { callee, .. } if matches!(&callee.kind,cgx2_syntax::ExprKind::Name(p) if !module.functions.iter().any(|f|f.name==p.segments.join(".")) && matches!(p.segments.join(".").as_str(),"length"|"get"|"updated"|"append"|"array"|"list"|"concat"|"slice"|"parse_i64"|"show")) => {
                    found.set(true)
                }
                _ => {}
            };
            cgx2_syntax::visit_stmts(&module.statements, &mut |_| {}, &mut visitor);
            for f in &module.functions {
                cgx2_syntax::visit_stmts(&f.body, &mut |_| {}, &mut visitor);
            }
            found.get()
        }
}

/// Canonical binder-qualified type identity for checked metadata.
pub fn type_key(ty: &Type) -> String {
    match ty {
        Type::Parameter { name, .. } => name.clone(),
        Type::List(t) => format!("List<{}>", type_key(t)),
        Type::Array(t) => format!("Array<{}>", type_key(t)),
        Type::Variant { name, arguments } | Type::AppliedRecord { name, arguments }
            if !arguments.is_empty() =>
        {
            format!(
                "{name}<{}>",
                arguments
                    .iter()
                    .map(type_key)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        _ => ty.to_string(),
    }
}
/// Substitute arguments into a body-checked signature. Admission/constraint
/// validation belongs to the checker, not to this metadata projection.
pub fn instantiate_signature(signature: &FunctionSignature, args: &[Type]) -> FunctionSignature {
    let map = signature
        .type_parameters
        .iter()
        .zip(args)
        .filter_map(|(p, t)| {
            if let Type::Parameter { name, .. } = p {
                Some((name.clone(), t.clone()))
            } else {
                None
            }
        })
        .collect();
    FunctionSignature {
        type_parameters: vec![],
        parameters: signature
            .parameters
            .iter()
            .map(|t| nominal::substitute(t, &map))
            .collect(),
        return_type: nominal::substitute(&signature.return_type, &map),
        public: signature.public,
        span: signature.span,
    }
}
