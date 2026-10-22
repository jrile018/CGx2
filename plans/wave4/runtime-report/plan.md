# CGx2 Memory Runtime Report and Conformance Plan

> Implement this plan in your attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Make the existing managed-memory runtime observable and verify its most important `no_gc` and region guarantees under deterministic tests.

**Architecture:** Extend `cgx2-runtime::Session` with a structured snapshot and a stable text/JSON report, using existing `RegionStats`. Include one row per open/closed region with mode, configured limit, current/peak/collected bytes, failures, collection count, and measured pause samples. Collection timing is observed by a monotonic clock around the actual collector; report format never hides missing samples. Maintain single-session safety and no new GC scheduling behavior.

**Tech stack:** Rust 1.98, standard library, current runtime crate only. No `unsafe`, external dependencies, or real-time promises.

**Specification:** `docs/language-design.md` sections “Values and regions” and “Choosing when collection runs”; `plans/wave4/README.md`.

## Owned paths and boundaries

- Own only `crates/cgx2-runtime/**` and this plan. Do not edit CLI, syntax, sema, benchmark, notebook, Python bridge, root manifests or shared docs.
- Preserve `Session::new`, `default_region`, `create_region`, `alloc_bytes`, `collect`, `stats`, owned-buffer APIs, and error semantics. New API should be additive: `Session::memory_report() -> MemoryReport` (or a documented equivalent) with deterministic region order and no mutation/collection while reporting.
- Use `RegionId` as the identity. Closed regions must remain visible as closed with terminal counters; invalid handles must not become valid when indices are reused. If existing internal storage requires a different safe representation, document it.
- Pause distribution can expose count, total, min, max, and sorted nearest-rank p50/p95 or raw samples; define exact units as nanoseconds and use checked/saturating arithmetic. Wall-clock timing varies, so tests assert internal consistency and count, not speed thresholds.

## Review focus

1. No collection count changes during `no_gc`, including denied explicit collection, failed allocation, and nested guards.
2. Allocation that exceeds a guard budget or region limit leaves accounting and object graph unchanged apart from the failure counter; no partial object is visible.
3. Manual regions collect only on explicit request; auto regions collect only at documented safepoints. Closed regions have stable terminal report rows.
4. Report JSON escapes names or avoids user-supplied names entirely; repeated snapshots with no operations are equal except for explicitly documented wall-time fields (prefer none).
5. Cross-region actions remain isolated; report totals match per-region counters without double counting foreign/Python memory.

## Tasks

### 1. Structured snapshot

- [ ] Add red tests for default/open/closed region rows, stable order, sums, and no mutation during snapshot.
- [ ] Add public `MemoryReport`/`RegionReport` types and `Session::memory_report`, with explicitly named byte and nanosecond fields. Document whether metadata is included in byte counters.
- [ ] Keep report creation bounded; if retaining every pause sample could grow without bound, use a documented fixed-size histogram or cap with a dropped-sample counter.

### 2. Collection timing and formatting

- [ ] Add collection timing around real collector invocation, including zero-live-object cycles. Record only completed collections. A forbidden request inside `no_gc` must not add a sample.
- [ ] Add deterministic `to_text` and `to_json` rendering (or separate functions) with versioned keys. Test exact schema and parseability without adding an unavailable dependency; a small local parser/test helper is acceptable.
- [ ] Include mode, limit, open/closed state, live/committed/peak bytes, allocation failures, collection count, total reclaimed bytes, and pause distribution.

### 3. Memory conformance regression

- [ ] Add tests for nested guards across two auto regions, explicit collect rejection, region-limit and budget failure atomicity, manual-versus-auto behavior, and stale handles after close.
- [ ] Run `cargo test -p cgx2-runtime --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-runtime --all-targets --offline -- -D warnings`.
- [ ] Update the crate README with report fields, exact limitations, and how a later CLI can display it. Commit only owned paths.

**Acceptance:** Reporting reflects actual CGx2 runtime activity without collecting or changing state; tests demonstrate zero collection in guarded sections and stable failure accounting. This does not establish whole-program memory safety until source and runtime are connected.
