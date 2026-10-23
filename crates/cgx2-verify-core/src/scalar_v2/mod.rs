//! Version 2 scalar proofs: eager lexical bindings and checked i64 multiplication/division.
//! The original root API remains frozen. Division truncates toward zero; zero
//! divisors and MIN / -1 use the legacy undefined-arithmetic `Overflow` outcome.
mod audit;
#[path = "../bounded.rs"]
mod bounded;
mod eval;
mod expr;
#[path = "../model.rs"]
mod model;
mod smtlib;
#[path = "../solver.rs"]
mod solver;
pub use audit::{
    audit_encoding, audit_encoding_with_source, AuditError, EncodingAudit, SourceEvaluator,
    OBLIGATION_NAMES,
};
pub use bounded::{verify_bounded, Counterexample, Domain, Failure, Scope, UnknownReason, Verdict};
pub use eval::{eval, EvalError, Value};
pub use expr::{Contract, Expr, Parameter, Sort, ValidationError};
pub use model::validate_solver_witness;
pub use smtlib::{
    check_smt_limits, emit_smtlib, emit_smtlib_with_limits, emit_witness_smtlib,
    emit_witness_smtlib_with_limits, SmtError, SmtLimits, SMT_DEPTH_LIMIT, SMT_NODE_LIMIT,
    SMT_OUTPUT_LIMIT,
};
pub use solver::{
    observe_solver, parse_solver_result, run_solver, SolverExit, SolverObservation, SolverResponse,
    SOLVER_MEMORY_LIMIT_BYTES, SOLVER_OUTPUT_LIMIT,
};

mod expansion;
pub use expansion::{ExpansionError, LiteralAccumulator, EXPANSION_CAP};

const ARITHMETIC_ASSUMPTION: &str = "checked i64 add/sub/mul/div and eager lexical bindings; overflow or invalid division is a contract failure";
