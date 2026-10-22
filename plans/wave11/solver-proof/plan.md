# Solver proof protocol and validated counterexamples

> Work in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Improve the existing external JSON contract verifier from finite enumeration plus untrusted solver observation toward a trustworthy bounded SMT result for its supported pure integer/Boolean subset. Keep proof status honest when no real solver is installed.

**Architecture:** Emit separately identifiable verification conditions for satisfiable preconditions, existence of a violating postcondition, and checked arithmetic safety. Parse solver statuses/models with strict bounds; re-evaluate any `sat` candidate against the original contract and i64 semantics before reporting `disproved`. A `proved` verdict may be emitted only when a configured, version-pinned real solver returns the required UNSAT results under the documented encoding and every query/status is complete; arbitrary user-supplied fake solver protocol responses remain `untrusted_protocol`. If trusted solver attestation cannot be established in this wave, keep proof result `unknown` and ship the parser/validation foundation instead.

**Tech stack:** Rust 1.98; `cgx2-verify-core`, `cgx2-verify-cli`; offline Cargo, optional external Z3/CVC5 installation. Neither is installed on the coordinator host at dispatch.

**Spec:** `docs/language-design.md` bounded verifier semantics; `docs/validation-plan.md` proof suite and reporting rules.

## Constraints

- Own only `crates/cgx2-verify-core/**` and `crates/cgx2-verify-cli/**`. Read but do not edit parser/sema/CLI source, runtime, root manifests/lock, docs/plans. This lane continues to accept external JSON v1 contracts; `.cg` source `cgx2 verify PATH` integration waits for the source interface lane.
- Preserve bounded input/output/time limits, subprocess cleanup, and exact signed i64 checked arithmetic. A `sat` model is a candidate until parsed and re-evaluated; an inconsistent precondition is `unknown` or a distinct vacuity status, never a meaningful proof. Unsupported foreign calls, arrays, loops, timeouts, malformed output and missing solver remain `unknown`. Never promote `unsat_protocol_only` from a fake executable to `proved`.
- Include known true/false contracts and adversarial fake solver cases: contradictory statuses, malformed model, out-of-range integer, missing assignments, extraneous output, timeout, crash, and output flood. Pin the real solver command/version/hash if a local solver becomes available; record exact installation and optional test status.

## Tasks and checks

1. Add failing model-parser/re-evaluation and trust-gate tests before implementation. Specify condition ordering and result JSON versioning in owned README.
2. Implement bounded status/model validation and any eligible real-solver trust path. Where a real solver is absent, test generated SMT syntax structurally and report the live proof test skipped, not passed. Retain finite-domain verdict as a separately labeled result.
3. Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-verify-core -p cgx2-verify-cli --offline`, format, targeted Clippy `-D warnings`, and available real-solver examples. Commit owned paths and report exact proof/unknown scope, checks, solver availability and SHA.

**Acceptance:** A fake solver cannot manufacture a proof or counterexample; every emitted status states whether it is finite, solver-backed, validated, or unknown.
