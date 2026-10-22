# Trace managed object references safely

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Extend the direct runtime with a small managed object graph profile so tracing reaches objects through checked references, while keeping byte-buffer and ownership behavior stable. This is a runtime prerequisite for typed records, not source-level typed heap objects yet.

**Architecture:** Add an opt-in object allocation shape with a bounded number of reference slots and byte payload, using stable `Session` handles and per-region tracing. Define exactly which handles are roots and which edges are strong. A slot update validates owner, target liveness, region compatibility, and `no_gc`/allocation state before changing the graph. No raw pointer or implicit cross-region edge may enter the safe API. Existing `Session` methods and memory reports retain their semantics; new public methods are additive.

**Ownership:** Only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Do not edit the compiler, source grammar/checker, FFI, root manifests/lock, docs or plans. The parallel native compiler uses the old API and must continue to compile unchanged.

## Required behavior

1. Write a reference-state model before implementation. Cover roots, cycles, shared children, cleared/overwritten slots, stale handles, region close, manual collection, auto threshold, and nested `no_gc` with cumulative budgets. Define whether and how an edge between regions is rejected. Bound slots and payload size and return a typed error for overflow/exhaustion without partial mutation.
2. Add deterministic state-machine/fault-injection tests that compare runtime transitions and per-region reports with the model. Include failure paths where a slot update or allocation fails, and prove by observed counters that no collection occurs inside `no_gc`; manual regions only collect explicitly. Check that collection of region A cannot invalidate any live object in region B.
3. Run existing v1–v5 conformance unchanged. Do not claim universal safety from finite seeds; record seed counts, skipped Miri/sanitizer runs, and any host allocation caveat. Keep foreign memory outside CGx2 region accounting.
4. Document new method names, parameters/defaults, rooting, edge semantics, and limits in owned READMEs.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline`, `cargo fmt --all --check`, and targeted Clippy `-D warnings`. Commit owned paths, report SHA, state-machine coverage, error atomicity, and unresolved release audit work.

**Acceptance:** Reachable graph objects survive collection, unreachable cycles can be reclaimed, and bad edges or failures cannot create dangling references or violate region/`no_gc` rules.
