# Pure immutable record cells in native notebooks

**Goal.** Let `.cgnb` cells retain and rerun Wave 21's immutable nominal scalar records without hidden managed allocation or a weaker pure execution boundary.

**Ownership.** Edit only the five `crates/cgx2-notebook-*` crates, their fixtures, and examples. Consume but do not edit Wave 21 syntax/checker/CLI record APIs, `cgx2-package`, root Cargo files, or shared docs.

## Deliverable and contract

1. Support local `record Name:` definitions with one to eight `i64`/`Bool` fields, named construction, field reads, and immutable bindings across pure cells. Preserve declaration-order fields and source-order argument evaluation. Use committed `Value::Record`; do not add mutable, nested, generic, managed, or package-imported records, or a notebook-only syntax. Wave 21 `module.function(...)` imports remain pure scalar.
2. Extend the worker/kernel result protocol with a bounded tagged record encoding that preserves record name, field names/order/types, and exact scalar values. Reject malformed lengths, extra fields, invalid UTF-8, and oversized payloads before allocating unbounded storage or publishing state. Keep subprocess timeout, cancellation, step/depth budgets, and effects/capabilities checks.
3. Make retained-state replay, saved output, source hashes/provenance, stale dependency tracking, selective rerun, and Restart and Run All handle local definitions and values consistently. Editing/deleting a definition invalidates dependents; stale saved values cannot appear fresh after reopen. Preserve portable markdown images and exact sidecar replacement.
4. Test two record definitions, reordered named arguments, a field read in a later cell, selective rerun after editing a definition, save/reopen/clean sweep parity, and a deliberate failure that preserves prior valid state. Diagnose unknown/duplicate/missing field, wrong type, mutation, use before definition, and imported-record boundary with cell ID and source span. Exercise CLI and loopback UI.

## Verification and handoff

Run offline tests for all five notebook crates, loopback UI tests where binding is permitted, `cargo fmt --all --check`, and owned Clippy `-D warnings`. Commit only owned paths; report SHA, example `.cgnb`, commands/results, provenance behavior, and unsupported imported/foreign/effectful profiles. Stop for review.
