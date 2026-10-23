# Executable bounded-proof trust boundary

**Goal.** Make it harder for a solver transcript, timeout, malformed model, or unsupported source feature to be reported as a trusted proof. Wave 17's finite encoding audit is evidence for its supported fragment, not proof of the full language.

**Ownership.** Edit only `crates/cgx2-verify-core/**` and `crates/cgx2-verify-cli/**`. Do not edit source compiler, memory runtime, root Cargo files, shared docs, or another lane.

## Deliverable

1. Define and enforce a narrow result state for supported, successfully checked obligations: proven, counterexample with independently validated witness, unknown, unsupported, or solver failure. Preserve a clear `unknown` for untested memory, foreign, concurrency, and arbitrary source behavior. No `unsat` response alone may promote a wider source file to globally verified.
2. Exercise solver-process handling with controlled fake executables: unexpected output, truncated output, contradictory/multiple `sat`/`unsat`, nonzero exit, timeout, huge output, and stale/incorrect model. Refuse unsafe promotion and preserve exact provenance (solver command/version if observable, obligation hash, bounds, and timeout). Use a nonzero bounded timeout and output cap.
3. If Z3 or CVC5 is locally available, run one actual supported true obligation and one false obligation end to end, compare the false witness with CGx2's independent evaluator, and record commands/results under owned paths. If unavailable, mark the live-solver gate unrun; mocked transcripts are only process-contract tests.
4. Add a regression case for an unsupported source or contract that can otherwise appear reassuring in the CLI. Update owned documentation with exact trust boundary and reproducible commands.

## Verification and handoff

Run offline verify-core and verify-cli suites, fake-solver adversarial tests, formatter, and Clippy on owned packages. Do not alter global proof policy beyond the bounded supported fragment. Commit only owned paths. Report commit SHA, files, actual versus mock solver evidence, exact timeout/output bounds, and remaining proof gaps; stop for review.
