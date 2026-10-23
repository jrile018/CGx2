# Faulted memory lifecycle assurance

**Goal.** Strengthen the highest-priority memory gate by testing collector and root behavior against an independent, bounded model under failures. Use Wave 17's direct root census as observed state.

**Ownership.** Edit only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Do not edit CLI/emitter, root Cargo files, shared docs, or another lane.

## Deliverable

1. Build or extend a deterministic reference-state test over a bounded set of regions, byte objects, owned objects, managed handles, duplicate roots, edges, collection modes, explicit collection, region close, and nested `no_gc`. Compare census (`rooted_handles`, `root_registrations`), liveness, and stable memory counters after each operation. State exactly which operations and graph bounds are modeled.
2. Inject allocation-reserve failures at controlled one-based points in several representative traces. Assert that failed operations leave no phantom root registration, no half-written edge, no unaccounted bytes, and no loss of the primary error. Check collection disabled/enabled timing only where contractually defined. Keep all randomized seeds and step limits fixed in committed fixtures and make failure traces replayable.
3. Add at least one test in which multiple registrations protect one handle, and a cycle is reclaimed only after its final external root is released and collection is permitted. Verify region-local and global census agree. If an actual runtime defect is found, fix it with the shortest failing trace as a regression test. Keep the direct-runtime result separate from source/native and foreign safety claims.
4. Update only the owned conformance report with exact command, result, seeds, traces, and any sanitizer evidence actually run. List unrun ASan/Miri/LSan or concurrent execution explicitly.

## Verification and handoff

Run targeted offline runtime and conformance suites twice, the deterministic replay, formatter, and Clippy on owned packages with `-D warnings`. Keep generated test work bounded so the suite remains usable. Commit only owned paths. Report commit SHA, file list, tested seeds/fault points, exact results, and residual safety gaps; stop for review.
