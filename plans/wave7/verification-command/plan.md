# CGx2 Bounded Verification Command Plan

> Implement this plan in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Turn the existing typed bounded verifier into a usable, reproducible command while preserving the difference between finite-domain proof, counterexample, and unknown. Prepare a safe solver path without treating protocol text or an absent solver as proof.

**Architecture:** New `cgx2-verify-cli` reads one versioned JSON contract document, validates a limited expression grammar, constructs `cgx2_verify_core::Contract` and `Domain` values, and calls `verify_bounded`. It renders stable text/JSON verdicts with exact finite scope and input hash. An optional `--solver PATH` emits the existing SMT-LIB query and records solver identity, transcript status, timeout, and assumptions as a separate experimental result; only the bounded verifier's verdict is a `Verdict` unless a trusted model/proof-validation layer is built and tested.

**Tech stack:** Rust 1.98, existing `cgx2-verify-core`, offline JSON dependency already in the workspace, std process APIs.

**Specification:** `crates/cgx2-verify-core/README.md`, `docs/validation-plan.md` proof gate, `plans/wave7/README.md`.

## Owned paths and boundaries

- Own only `crates/cgx2-verify-core/**`, new `crates/cgx2-verify-cli/**`, and this plan. Do not change `.cg` syntax, sema, CLI, runtime, root files, or other lanes. This command reads an external typed contract document; source `requires`/`ensures` remain later integration work.
- Command: `cgx2-verify CONTRACT.json [--format text|json] [--max-cases N] [--solver PATH] [--timeout-ms N]`. Give bounded defaults (for example `max_cases=100000`, `timeout_ms=2000`) and hard caps documented in README. Require explicit finite integer domains for each i64 parameter; reject extra/missing domains, duplicate names, oversized JSON, non-integer numbers, and unsupported expression nodes before verification. No unbounded exhaustive enumeration.
- JSON schema version 1 uses tagged nodes for Boolean/i64 literals, variables, checked `+`/`-`, comparisons, Boolean operations, and explicit uninterpreted calls. Include the exact allowed tags/types and a sample valid, false, inconsistent, and unsupported contract. Generated/parsed expressions must pass `Contract::validate`.
- Bounded `Proved` means only every enumerated input in the displayed scope passed and at least one input satisfied `requires`. A false case returns a reevaluated assignment. Inconsistent preconditions, exhausted limits, unsupported foreign calls, and missing solver are `Unknown` with reasons. Preserve the mathematical-Int-vs-checked-i64 assumptions of SMT emission.
- If `--solver` is provided, never infer a theorem solely from three status lines. Record executable path/version, script hash, statuses, stderr/timeout, and `untrusted_protocol` or equivalent status. A `sat` transcript is a candidate until parsed and reevaluated. Bound child output and ensure timeout terminates process/readers, including a malicious or stalled solver. Do not install Z3 or use a fake solver as real proof evidence.

## Tasks

### Input and bounded result

- [x] Add CLI tests for valid bounded proof, false postcondition with counterexample, inconsistent precondition, case cap, uninterpreted call, malformed/oversized input, and stable JSON.
- [x] Implement versioned parser, safe limits, verdict formatter, and source/input hash. A repeated input produces byte-identical deterministic output except for explicitly documented solver metadata.
- [x] Add README examples runnable offline without an external solver.

### Solver transport boundary

- [x] Add fake-solver tests for `unsat`, `sat`, `unknown`, malformed output, nonzero exit, stderr, timeout, and large/stalled output. Fake responses appear as untrusted protocol observations.
- [x] Repair `run_solver` transport limits without changing public verdict semantics. No real solver was found on this host; the live gate remains open.

### Verification and handoff

- [x] Run `cargo test -p cgx2-verify-core -p cgx2-verify-cli --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-verify-core -p cgx2-verify-cli --all-targets --offline -- -D warnings`.
- [x] Commit only owned paths; show a proof-scope example and state the missing source integration and live-solver trust gates.

**Acceptance:** An external contract can be checked reproducibly with honest finite-domain results and solver observations that cannot be mistaken for a global proof.
