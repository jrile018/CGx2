mod check;
mod diagnostic;
mod ir;

/// Bounded function/CFG ownership analysis; independent of the legacy operation IR.
pub mod v1;

pub use check::check_ops;
pub use diagnostic::OwnershipDiagnostic;
pub use ir::Op;

/// Additive conditional-owner CFG and worklist checker.
pub mod v2;

/// Additive two-outcome transfer ownership and region quantification.
pub mod v3;
