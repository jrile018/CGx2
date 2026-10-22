# Automatic collection assurance

Use the attached worktree; read shared Wave 15 references and this plan. Commit only owned paths and stop for review.

**Goal.** Wave 14's graph model used an effectively disabled auto trigger. Exercise actual automatic collection safepoints with reachable graphs, cycles, faults, and `no_gc`, and document the evidence against the hard memory release gate.

**Ownership.** Only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Preserve the public `Session` graph API for the parallel compiler lane. Do not change CLI, emitter, foreign adapter, root Cargo files, or plans.

## Deliverable

1. Add a deterministic, bounded state-model corpus where auto regions cross their real collection threshold. Include rooted parent/child, cleared edge, cycle, stale handle, owned promotion/copy, manual region alongside auto region, and nested `no_gc`. Check liveness, per-region and total reports after each operation; print seed and minimized trace on failure. Specify seed count, memory cap, object sizes and time bound.
2. Instrument or expose the minimum test-only controls needed to prove that allocation safepoints collect only in the intended auto region and never inside `no_gc`, without changing public behavior. Compare actual collection counts to model expectations. A manual-region limit failure must still take precedence over a host reservation fault when knowable.
3. Re-run fault injection across object reservation, promotion, edge growth and collection-trigger boundaries. Verify rollback/atomicity, subsequent valid use and exactly-once release. Fix a demonstrated bug, if found, with a regression case. Do not add tests that merely repeat implementation branches.
4. Update the owned release-gate report to separate direct-runtime evidence, source/native parity dependency, foreign lifetimes, thread safety and sanitizer availability. Run Miri/sanitizer only if a compatible toolchain exists and report exact command and result; otherwise mark unrun.

## Verification

Run offline tests of both owned crates twice for deterministic seed evidence, targeted format/Clippy `-D warnings`, and any available dynamic-tool command. Report exact collection thresholds/counters, error precedence, model bounds, unresolved gates and commit SHA.

**Acceptance.** Automatic collection and `no_gc` behavior have reproducible graph/fault evidence. The global memory gate remains open unless every dependent source/native/foreign gate is actually satisfied.
