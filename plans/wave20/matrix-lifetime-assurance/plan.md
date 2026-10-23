# Matrix facade lifetime assurance

**Goal.** Increase confidence in the direct CPU matrix ABI and its ownership rules while the source lane binds to it.

**Ownership.** Edit only `crates/cgx2-bind/**` and `crates/cgx2-libtorch/**`. The committed v2 facade ABI, symbol names, layout IDs, ownership contract, and schema-2 metadata are frozen for Wave 20. Do not edit core source, compiler, runtime, notebook, Python emitter, shared docs, or root Cargo files.

## Deliverable

1. Add a bounded deterministic lifecycle corpus that creates distinct rank-2 input handles, multiplies, copies results, releases in varied orders, and repeats under ordinary and fault paths. Include dimensions at each allowed boundary, incompatible inner dimensions, too-small output capacity, checked element-count overflow, wrong facade version, and missing required symbol. Capture exact expected error classes; no out-of-bounds read/write or double-release may be considered acceptable.
2. Verify the copied pinned LibTorch package and every loaded ELF dependency before a real test. Continue to check that the adapter has no mapped `libpython` dependency. Report the exact Torch package pin, facade hash, host architecture, loader closure, command, and result. Keep mock ABI tests clearly distinct from the real CPU test.
3. Run an available dynamic tool against the host facade or Rust-to-C boundary where technically possible, recording what it instruments. If no dynamic tool can instrument the copied binary, provide reproducible attempt and leave the boundary safety gate open. Avoid broad GPU, arbitrary Python package, or ten-language compatibility claims.
4. Make only additive internal assurance changes. If a v2 ABI bug requires an incompatible change, write a failing test and a proposed v3 contract in an owned report, then stop without silently changing the source lane's interface.

## Verification and handoff

Run `cargo test --offline -p cgx2-bind -p cgx2-libtorch`, existing preflight, focused real pinned matrix test if package exists, `cargo fmt --all --check`, and owned Clippy `-D warnings`. Commit only owned paths; report SHA, real versus mock evidence, dynamic-tool result, and residual foreign-lifetime risks. Stop for review.
