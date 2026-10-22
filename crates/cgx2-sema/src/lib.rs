//! Baseline static semantics for the Wave 1 CGx2 syntax tree.

mod check;
mod diagnostic;
mod effects;
mod memory;
mod types;

pub use check::{
    check_module, check_module_with_imports, collect_public_signatures, FunctionSignature,
    ImportedSignatures, SpanTypes, TypedModule,
};
pub use diagnostic::TypeDiagnostic;
pub use effects::EffectSummary;
pub use types::Type;
