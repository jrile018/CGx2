//! Explicit, versioned input for a future typed compiler lowering.
//!
//! This module checks ownership, not types, field existence, effects, runtime
//! allocation, or source syntax. Summaries are trusted contracts: the caller must
//! check each local body against its summary and authenticate foreign summaries.
//! See the crate's `ADAPTER.md` for the fail-closed lowering contract.

mod analysis;
mod model;

pub use analysis::analyze_function;
pub use model::*;
