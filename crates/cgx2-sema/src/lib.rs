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
