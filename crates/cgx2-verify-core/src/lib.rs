//! Standalone verification conditions for CGx2's pure Boolean and checked-i64 subset.

mod audit;
mod bounded;
mod eval;
mod expr;
mod model;
mod smtlib;
mod solver;

pub use audit::{
    audit_encoding, audit_encoding_with_source, AuditError, EncodingAudit, SourceEvaluator,
    OBLIGATION_NAMES,
};
pub use bounded::{verify_bounded, Counterexample, Domain, Failure, Scope, UnknownReason, Verdict};
pub use eval::{eval, EvalError, Value};
pub use expr::{Contract, Expr, Parameter, Sort, ValidationError};
pub use model::validate_solver_witness;
pub use smtlib::{emit_smtlib, emit_witness_smtlib, SmtError};
pub use solver::{
    observe_solver, parse_solver_result, run_solver, SolverExit, SolverObservation, SolverResponse,
    SOLVER_MEMORY_LIMIT_BYTES, SOLVER_OUTPUT_LIMIT,
};
