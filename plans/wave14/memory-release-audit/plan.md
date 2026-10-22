# Memory release audit

Use the attached worktree. Read the shared Wave 14 references and this plan before coding. Commit only owned paths and stop for user review. Memory correctness has priority over throughput claims.

**Goal.** Exercise the frozen Wave 13 managed object graph under allocation, collection, fault injection, manual-region limits, `no_gc`, and stale-handle transitions. Produce a reproducible statement of what is proven and what still blocks release. Preserve the public `Session` graph API consumed by the source and native lanes.

**Ownership.** Only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Do not edit syntax, compiler, notebook, binding, benchmark, workspace manifests, plans, or shared docs. A needed cross-crate API is a handoff item, not a silent edit.

## Deliverable

1. Extend the deterministic state-model corpus with object slots and edges: root/unroot, set/clear reference, collect, close region, and nested `no_gc`. Include cycles and a live parent retaining a child. Compare runtime reports and liveness to a small explicit model after each step. Record seed and minimized operation trace on failure. Bound seeds, lengths, slots, and test time in code and README.
2. Add fault placement cases at object header/slot reservation, edge mutation if allocation is involved, collection worklist, and any host-reservation boundary actually supported by the runtime. Verify rollback or a documented atomic failure, exact error class, no leaked live handle, and no later double release. The manual-region limit must take precedence where the limit is already knowable.
3. Add one explicit release-gate report or README matrix covering interpreter graph semantics, native graph parity dependency, foreign tensor lifetime exclusion, thread-safety status, and sanitizer status. Do not mark a gate passed because a component test passes.
4. Keep error taxonomy and public method signatures stable for parallel consumers. If a bug requires a breaking change, stop at a regression test and explain the needed coordinator change.

## Verification

Run offline tests for `cgx2-runtime` and `cgx2-memory-conformance`, targeted format and Clippy, the seeded corpus twice with the same seed, and one small direct graph example. Report exact commands, seed count, failures fixed, remaining release risks, and commit SHA. Sanitizer runs are optional only when the required toolchain exists; distinguish unavailable from passed.

**Acceptance.** Reproducible graph/fault evidence covers the frozen API and no test hides a failed allocation or use-after-release. The global memory release gate may still remain open.
