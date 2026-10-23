# Pure notebook function cells

**Goal.** Make small `.cgnb` experiments more modular by retaining safe pure user functions across cells while preserving Wave 16's effect boundary and bounded interruption behavior.

**Ownership.** Only the five `crates/cgx2-notebook-*` crates. Consume frozen checker/runtime APIs and ordinary `.cg` syntax. Do not edit compiler/CLI, root Cargo files, shared docs, or other lanes.

## Deliverable

1. Admit an explicitly bounded subset of pure local function definitions and calls with the same scalar semantics as `cgx2 run`. The parser rejects function items mixed with top-level statements, so build a bounded function-only module with a generated temporary `main` wrapper for each evaluating cell, or document an equally safe use of frozen APIs. Reserve `main` against user collision and enforce cumulative source/step/call-depth limits. State admitted parameter/return types; reject cross-cell capture of `let` bindings and unsupported recursion. Persist definitions in the live session only, without replaying earlier cells. File/network/foreign/collection effects, unsafe/native/Python declarations and capability use remain rejected before execution.
2. Use function symbol dependencies to mark saved output stale when a definition changes, is removed, fails, times out, or is interrupted, including transitive callers. Rerunning a dependent cell must use the current definition; independent completed cells retain their revisions. Reopen saved `.cgnb` without execution and preserve opaque-effect stale propagation from Wave 16.
3. Add a multi-cell fixture with definition, dependent and independent calls, edit/removal, a `main` collision, rejected effect/foreign declaration, interrupt during a pure call, restart, save/reopen, CLI and UI status. Keep source/output limits and worker timeout intact. If the frozen APIs cannot safely retain function definitions, provide explicit rejection tests and a concrete implementation blocker instead of claiming support.

## Verification

Run targeted offline notebook tests, CLI round-trip, UI loopback where socket permission exists, formatter and Clippy with warnings denied. Report admitted syntax, state transitions, skips and commit ID. Stop for review.
