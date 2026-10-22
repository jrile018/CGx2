# Direct source-level tensor call

Use the attached worktree; read shared Wave 14 references and this plan. Commit only owned paths and stop for review. This is a direct native LibTorch route; it must not invoke Python, CPython, PyTorch's Python module, or a Python bridge.

**Goal.** Give `.cg` one honest, narrow way to create CPU one-dimensional float64 tensor values, add equal-shaped tensors, and copy the result back, using the frozen Wave 13 `cgx2-libtorch` facade. Optimize for readable source and source-located errors; an unsafe foreign boundary remains explicit.

**Ownership.** Only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, and `crates/cgx2-api-index/**`, including each crate's Cargo manifest. Consume the frozen adapter API; do not edit `cgx2-libtorch`, `cgx2-bind`, root workspace files, docs, or another lane. If adding a direct crate dependency changes `Cargo.lock`, leave root lock resolution for coordinator integration.

## Deliverable

1. Choose one small source spelling for a bounded literal float64 vector and explicit unsafe native tensor operations. Document it in an owned README and fixture. The syntax must have exactly one parse and formatter form. Reject NaN/Infinity or define precise round-trip semantics; bound vector size and nesting. Do not claim a general numeric tower or arbitrary Tensor type.
2. Type-check rank, element type, shape compatibility when statically known, capabilities/effects, explicit unsafe call, and lifetime/escape restrictions. A tensor handle may not survive its adapter, cross adapter instances, enter managed graph slots, or silently copy. Static diagnostics must point to `.cg` source. If those restrictions cannot be enforced in this lane, narrow to a single lexical expression that keeps all handles local.
3. Wire `cgx2 check` and `cgx2 run` through the native adapter and an exact package lock. Propagate foreign errors without fallback. Preserve the existing scalar native C and Python-only routes. API index identifies the unsafe native effect and bounded profile.
4. Test with the existing mock facade and a negative missing/mismatched artifact. Include a real-LibTorch test only when a compatible artifact is installed; never label mock success as PyTorch conformance. State that the parallel closure lane's stronger pinning is an integration dependency, not an assumed feature.

## Verification

Run offline syntax/sema/CLI/API-index tests, format, targeted Clippy with warnings denied, one direct `.cg` source smoke and rejection fixture. Report exact source spelling, what bytes cross the C boundary, handle ownership, tests skipped, and commit SHA.

**Acceptance.** A supported `.cg` program can execute the direct CPU tensor profile without Python, while unsupported shapes, unsafe omissions, and lifetime escapes fail clearly. Real LibTorch success remains unproven until run with its actual binaries.
