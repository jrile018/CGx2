# CGx2 bounded verification core

This standalone Rust crate accepts a typed `Contract` from a future compiler adapter. It does not parse `.cg` files. Its finite verifier covers Boolean values, checked `i64` addition/subtraction, equality, integer ordering, `and`, `or`, and `not`. Both operands of Boolean operators are evaluated, so checked arithmetic in either operand can fail. A postcondition overflow is a contract failure with a counterexample; a precondition overflow is `Unknown`.

`verify_bounded(&contract, &domains, max_cases)` enumerates `Domain { name, min, max }` inclusive for each `i64` parameter. Boolean parameters automatically range over `false, true`. It returns `Proved` only after every input combination has been checked and at least one satisfies `requires`. `Scope` records the exact domains, Boolean parameters, case count, and the checked arithmetic assumption. A false postcondition or overflow returns `Disproved` with an assignment reevaluated against the contract. Invalid contracts, empty/missing domains, inconsistent preconditions, uninterpreted calls, and an exhausted case cap return structured `Unknown` reasons. A bounded `Proved` result says nothing about inputs outside the listed ranges.

`emit_smtlib(&contract)` returns an SMT-LIB script using mathematical `Int` constrained to signed `i64`. Addition and subtraction generate range guards. The script issues three `check-sat` queries in order: possible precondition overflow; satisfiability of a defined precondition; then satisfiability of its negated postcondition or postcondition overflow. Expected proof transcript: `unsat`, `sat`, `unsat`. The middle `unsat` diagnoses an inconsistent precondition. External calls use uninterpreted, pure, total functions; each syntactic `i64` call result has an explicit range assumption in the script. No finite proof is attempted for these calls.

`parse_solver_result(stdout, stderr, exit_status)` accepts exactly those three status lines and exit code zero with empty stderr. It returns `SolverResponse::Unsat` for the proof-shaped transcript, `SatCandidate` for a possible counterexample, and `Unknown` for missing solvers, timeouts, malformed responses, solver `unknown`, invalid preconditions, and execution errors. Neither response is a `Verdict`: a caller must bind `Unsat` to this emitted script, a trusted solver and disclosed assumptions before claiming proof; a `SatCandidate` needs a parsed model reevaluated by the CGx2 semantics before `Disproved`. `observe_solver(executable, args, script, timeout)` supplies a bounded transport: 16 KiB retained per output stream, a deadline covering stdin/process/pipes, and process-group termination on Unix. `run_solver` keeps its protocol-only response API atop that transport. A later integration layer must handle model extraction and proof provenance.

The local environment has no `z3` executable. Protocol tests use a fake solver command; no live external SMT proof has been established here.

```sh
cargo test --manifest-path crates/cgx2-verify-core/Cargo.toml --offline
cargo fmt --manifest-path crates/cgx2-verify-core/Cargo.toml --check
cargo clippy --manifest-path crates/cgx2-verify-core/Cargo.toml --offline -- -D warnings
```
