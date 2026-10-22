# CGx2 Bounded Verification Core Implementation Plan

> **For agentic workers:** Use the executing-plans skill task by task. This chat owns one bounded deliverable; do not spawn another agent, edit shared files, merge, or start a second assignment. Stop for user review after verification.

**Goal:** Implement a standalone verification-condition core that gives sound proved/disproved/unknown results for explicitly bounded pure integer/Boolean contracts and emits equivalent SMT-LIB for later solver integration.

**Architecture:** A typed expression AST represents variables, checked `i64` arithmetic, comparisons, Boolean operations, and pre/postconditions. A finite evaluator enumerates caller-declared input domains up to a case cap; it proves only that bounded domain, returns a counterexample for a failure, and returns unknown for unsupported or incomplete exploration. An SMT-LIB emitter encodes `i64` range/overflow rules and a process adapter parses `sat`/`unsat`/`unknown` without treating a missing solver as proof.

**Tech Stack:** Rust stable 1.98.1, standard library only. The local environment has no `z3` executable, so live SMT solving is a later integration gate.

**Spec:** `docs/language-design.md` contracts/proofs and `docs/validation-plan.md` proof suite.

## Global constraints

- Own only `crates/cgx2-verify-core/**` and `plans/verifier/plan.md`. Do not change compiler IR, shared docs, or root manifests.
- This crate accepts its own typed input; a later compiler adapter will translate CGx2 IR. No parser or Python/PyTorch proof claim in this slice.
- `Proved` is allowed only for a completely enumerated finite domain or an `unsat` result for the negated condition from a valid solver protocol. A timeout, missing solver, parse error, unsupported operation, or exceeded case cap is `Unknown`.
- Checked `i64` overflow is a defined failure; do not use wrapping host arithmetic. An inconsistent precondition is diagnosed instead of silently producing a vacuous proof.
- External calls are explicit uninterpreted assumptions and remain outside the finite evaluator.

## Public contract

Create `crates/cgx2-verify-core/Cargo.toml`, `src/lib.rs`, `src/expr.rs`, `src/eval.rs`, `src/bounded.rs`, `src/smtlib.rs`, `src/solver.rs`, `tests/contracts.rs`, `tests/smtlib.rs`, and `README.md`.

Expose `Sort::{Bool, I64}`, a typed `Expr` for literals/variables/checked add/subtract/comparisons/Boolean operators, `Contract { parameters, requires, ensures }`, `Domain { name, min, max }`, and `Verdict::{Proved { scope }, Disproved { counterexample }, Unknown { reason }}`. Public functions: `verify_bounded(contract, domains, max_cases)`, `emit_smtlib(contract)`, and `parse_solver_result(stdout, stderr, exit_status)`. `Proved.scope` lists every bounded input range or solver assumptions. `Unknown.reason` distinguishes cap, unsupported, timeout, solver unavailable, and malformed response.

## Review focus

1. An unsatisfiable `requires` clause must not be reported as proof of `ensures`.
2. `i64::MAX + 1` and `i64::MIN - 1` must not wrap in evaluation or SMT encoding.
3. Exceeding `max_cases` after partial enumeration must be unknown, even if all visited cases passed.
4. `sat` is a counterexample candidate requiring model validation before `Disproved`; malformed solver output is unknown.
5. A missing or timed-out solver must never become `Proved`.

## Tasks

### T1: Typed AST and checked evaluator

- depends_on: []
- owns: [`crates/cgx2-verify-core/**`]
- non_goals: [CGx2 parsing, loop invariant synthesis, foreign-call models]
- consumes: [proof subset in shared spec]
- produces: [typed contract/evaluator API]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Add failing tests for Boolean and `i64` variables, valid addition, overflow, sort mismatch, and invalid variable reference.
- [ ] Run `cargo test --manifest-path crates/cgx2-verify-core/Cargo.toml --offline` to observe missing behavior.
- [ ] Implement AST validation and checked evaluation; return typed errors instead of panics/wraparound.
- [ ] Run crate tests; document exact supported operators.

### T2: Exhaustive finite-domain proof

- depends_on: [T1]
- owns: [`crates/cgx2-verify-core/**`]
- non_goals: [claiming full unbounded `i64` proof]
- consumes: [typed evaluator]
- produces: [bounded `Verdict` with provenance]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Add failing tests for a valid bounded property, a smallest counterexample, impossible precondition, overflow case, empty domain, and case-cap exhaustion.
- [ ] Run tests to observe missing behavior, then implement deterministic enumeration with a checked Cartesian-product count.
- [ ] Return `Proved` only after all admitted inputs are explored; include exact ranges and checked-operation assumptions. Validate counterexamples by reevaluating the contract.
- [ ] Run crate tests and inspect each verdict's structured reason/scope.

### T3: SMT-LIB and solver protocol

- depends_on: [T2]
- owns: [`crates/cgx2-verify-core/**`, `plans/verifier/plan.md`]
- non_goals: [bundling/installing `z3` automatically]
- consumes: [typed contract AST]
- produces: [SMT-LIB text and safe solver-result parser]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Add failing snapshot tests for SMT-LIB `i64` bounds and overflow guards, plus protocol tests for `unsat`, `sat`, `unknown`, timeout, missing executable, and malformed output using a fake solver command.
- [ ] Run tests to confirm failures, then implement emission and protocol parsing. Require a validated solver response and disclosed assumptions before accepting an external proof.
- [ ] Run `cargo test --manifest-path crates/cgx2-verify-core/Cargo.toml --offline`, `cargo fmt --manifest-path crates/cgx2-verify-core/Cargo.toml --check`, and `cargo clippy --manifest-path crates/cgx2-verify-core/Cargo.toml --offline -- -D warnings`.
- [ ] Document the bounded proof guarantee and absence of local `z3`; commit owned paths and stop. Report test evidence, commit ID, and integration assumptions.

**Acceptance:** finite-domain truths are proved with explicit scope, false properties yield validated counterexamples, incomplete/unsupported cases remain unknown, checked overflow is preserved, and missing `z3` is reported honestly.
