# Allocation failure and graph invariant assurance

> Use the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Close the Wave 12 graph test's host-allocation failure gap with deterministic fault injection and expand error-path evidence for collection, roots, slots and `no_gc`.

**Architecture:** Add an opt-in test-only allocation-failure hook at the runtime's actual fallible reserve points, with an explicit operation counter and failure index. Default production behavior is unchanged. If a public test configuration API is needed, make it additive and impossible to enable accidentally in ordinary execution. The hook must not fabricate a successful allocation or bypass existing logical region/budget checks. Model the exact pre/post state for every injected failure and preserve the existing `Session` graph API used by the parallel source lane.

**Ownership:** Only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Do not edit source checker, emitters, FFI, notebooks, root manifests/lock, docs or plans. Preserve Wave 12 `Session::alloc_object`, `set_reference`, `reference`, report schema and error meanings.

## Required behavior

1. Enumerate actual reserve/allocation sites in byte and object paths; inject failure at each reachable site in focused deterministic tests. For each, assert error type, unchanged handle graph and root counts, no newly committed slot, report counters, retained ownership after failed conversion, and no collection where `no_gc` applies. If a host allocation cannot be injected without hidden Rust allocator behavior, document the uncovered site precisely.
2. Extend the independent graph state model with stale-handle reuse, overwritten/cleared edges, unreachable cycles, region close/isolation, auto safepoints, nested guards and both success/failure after a forced reserve error. Keep seed and operation counts recorded; the model must not merely mirror runtime internals.
3. Run all prior byte/owned and v1–v5 corpora unchanged. Add Miri or sanitizer checks only when the host/toolchain provides them, with exact command and skip reason. Never infer universal memory safety from finite tests.
4. Update owned READMEs/audit file with hook defaults, counts, failure semantics, skipped external tools and remaining release audit needs.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline`, format and targeted Clippy `-D warnings`. Commit owned paths and report SHA, injected failure sites, model seeds, collection counters and any untestable allocator path.

**Acceptance:** Every modeled failure leaves the runtime in a valid checked state, and the hard manual-region/`no_gc` counter rules hold on injected error paths.
