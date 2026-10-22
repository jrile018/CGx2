# Failure lifecycle parity

**Goal.** Wave 15 reports failure-time state, but its `rollback_preserved` flag does not include root counts and a local function can be cleaned up after the snapshot. Make that lifecycle observable and compare equivalent checked `.cg` programs across execution paths.

**Ownership.** Only `crates/cgx2-cli/**`, `crates/cgx2-emit-rust/**`, and `crates/cgx2c/**`. Consume the frozen Wave 15 runtime API; no runtime, checker, manifest, notebook, or shared-document edits. Keep existing JSON fields backward compatible; additive fields only.

## Deliverable

1. Define separate operation-failure and final-unwind snapshots, with rooted handles/edges, per-region and total counters, and collection count. Document precisely when local bindings are released. Avoid claiming transactional rollback of earlier successful operations.
2. Add actual source fixtures for nested local functions and top-level code covering stale handles, cross-region edges, manual limits, `no_gc` budgets, host allocation faults, and failure after a successful mutation. Compare interpreter with native debug and release outcomes wherever the feature profile allows. Assert error class/span, failure-time state, and final cleanup; identify unsupported cases explicitly.
3. Preserve failure diagnostics when report construction itself fails, and keep default success output unchanged. Any fault injection option must be test-scoped and unavailable in normal release behavior.

## Verification and handoff

Run offline tests for the three owned crates, formatter, targeted Clippy with warnings denied, and documented direct `cgx2c` fixtures in debug/release. Report exact parity matrix and commit ID. Stop for review.
