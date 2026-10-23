# Automatic collection pressure and failure-state model

**Goal.** Extend Wave 22's fixed graph sequence into an independent model that actually exercises automatic collection under allocation pressure, with varied operation structure and recoverable failures.

**Ownership.** Edit only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Do not edit `cgx2c`, CLI, syntax/checker, notebook, foreign adapters, root Cargo files, or shared docs.

## Deliverable and contract

1. Reuse the stable `Session`, root census, and report APIs. Model an auto and a manual region with thresholds low enough to force automatic collections. Independently derive reachability, committed/live/peak bytes, roots, collection/reclamation counters, and handle generations. Wave 22 used `trigger_bytes=4096`; varying payload bytes in the same short trace is insufficient.
2. Generate deterministic seed-replayable sequences with varied allocation/root/unroot/link/copy/collect/close order. Provide a bounded fixed seed corpus and an explicit larger opt-in run. Record actual transition and collection coverage. Print failing seeds and executable operation prefixes, with deterministic reduction or a documented minimal-reproducer procedure. Deliberately corrupt expected state in a test to prove the oracle detects divergence.
3. Cover rooted values surviving an auto collection, unreachable graphs reclaimed at the correct safepoint, manual counters unchanged without explicit collection, nested `no_gc` across both regions, budget failures without collection, retry after guard exit, and stale generations after slot reuse. Model each reserve-failure site relative to the automatic-collection safepoint: pre-collection failures must not collect, while a post-collection reserve failure may legitimately increment collection/reclamation counters. Neither may commit the failed allocation or corrupt surviving/unrelated state. Vary reserve sites and preserve first failure. Do not derive expected outcomes from runtime-returned outcomes.
4. Distinguish source and direct-runtime coverage. No source graph or concurrency syntax is added. Repair any demonstrated runtime defect within ownership with a minimal regression. Run available dynamic instrumentation on the new high-risk paths and record the instrumentation boundary and leak mode.

## Verification and handoff

Run runtime and memory-conformance suites twice, runtime tests with `--features fault-injection`, normal- and fault-feature model cases, and the expanded corpus with exact seed/count settings. Run formatting and owned Clippy `-D warnings`. Commit owned paths and an evidence report listing seeds, traces, achieved auto-collection coverage, commands/results, instrumentation limits, and remaining release blockers. A finite corpus and blocked LeakSanitizer do not establish general safety or leak freedom. Stop for review.
