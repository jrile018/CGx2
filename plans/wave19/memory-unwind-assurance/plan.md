# Memory unwind and cancellation assurance

**Goal.** Treat safe memory correctness as the first release blocker. Extend direct-runtime evidence to early returns, panics caught by the host, nested `no_gc` guards, and explicit root cleanup without claiming those tests prove all memory safety.

**Ownership.** Edit only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Read the Wave 18 root-census and state-model tests. Do not edit source compiler, foreign adapter, notebook, root Cargo files, or shared docs.

## Deliverable

1. Add a bounded replayable corpus of host-level early-exit and `catch_unwind` paths around region creation, managed allocations, duplicate roots, edge changes, nested `no_gc`, collection, and close. Record exact seed/operation traces and assert the census, accounting, liveness, and `no_gc` collection counters after each step. If a guard requires explicit cleanup rather than RAII, document and test that contract; do not invent automatic cleanup that would invalidate handles.
2. Test reserve failures before and during guarded operations. Ensure a failed allocation does not register a phantom root, charge bytes, or collect within `no_gc`; earlier successful mutations remain. A panic or failed operation must not silently disable the outer guard or underflow its budget. Preserve the first failure and report cleanup failures separately where the public API permits.
3. Run at least one available dynamic tool over the direct-runtime corpus (AddressSanitizer, Miri, or another appropriate tool) and commit the exact command, toolchain, scope, result, and exclusions in an owned report. If unavailable, record the attempted command and reason rather than calling a normal test a sanitizer pass. Fix any reproduced defect with its minimal replay trace.
4. Keep the conformance run deterministic and bounded on ordinary `cargo test`; a longer stress run may be an explicit ignored test. State that no source/native, foreign, or parallel-memory safety result follows from the direct-runtime corpus alone.

## Verification and handoff

Run `cargo test --offline -p cgx2-runtime -p cgx2-memory-conformance` twice, replay the shortest edge cases, `cargo fmt --all --check`, and Clippy for owned packages with `-D warnings`. Run the dynamic tool where available. Commit only owned paths; report SHA, files, seeds/fault points, exact commands/results, and residual safety gaps; stop for review.
