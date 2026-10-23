//! Baseline static semantics for the Wave 1 CGx2 syntax tree.

mod check;
mod diagnostic;
mod effects;
mod imports;
mod memory;
mod tensor;
mod types;

pub use check::{
    check_module, check_module_with_imports, collect_public_signatures,
    collect_public_signatures_with_imports, FunctionSignature, ImportedSignatures, SpanTypes,
    TypedModule,
};
pub use diagnostic::TypeDiagnostic;
pub use effects::EffectSummary;
pub use imports::CheckedImportedEffects;
pub use tensor::{tensor_profile, MatrixLiteral, MatrixLiteralF32, TensorOperation, TensorProfile};
pub use types::Type;

mod nominal;

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
