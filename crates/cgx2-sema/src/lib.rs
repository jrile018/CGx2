//! Baseline static semantics for the Wave 1 CGx2 syntax tree.

mod check;
mod diagnostic;
mod types;

pub use check::{check_module, FunctionSignature, SpanTypes, TypedModule};
pub use diagnostic::TypeDiagnostic;
pub use types::Type;
