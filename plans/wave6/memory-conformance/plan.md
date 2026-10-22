# CGx2 Deterministic Memory Conformance Corpus Plan

> Implement this plan in your attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Stress the frozen Wave 5 source/runtime memory path with reproducible valid and invalid programs, and make failures easy to reproduce by seed and source.

**Architecture:** New `cgx2-memory-conformance` crate contains a small reference model for region open/close, owned move/drop, managed root/drop, collection count, and nested `no_gc` budgets. A deterministic generator builds bounded `.cg` scripts from valid model operations, runs them through `cgx2_cli::run_source_with_report`, and compares invariants/counters. Separate invalid fixtures target exact diagnostic families. This is a conformance corpus, not a proof of all memory safety. Keep the generator independent of compiler implementation details beyond the public source/API contract.

**Tech stack:** Rust 1.98, existing CLI/runtime crates, standard library deterministic PRNG (document algorithm/seed), no external fuzz dependency.

**Specification:** `docs/validation-plan.md` memory hard gates, `plans/wave6/README.md` frozen Wave 5 profile.

## Owned paths and fixed boundaries

- Own only new `crates/cgx2-memory-conformance/**` and this plan. Do not edit syntax, sema, ownership, CLI, runtime, root manifests/lock, or benchmark harness.
- Generate only Wave 5 source forms: `memory.region(auto|manual)`, `owned_bytes`, `move`, `alloc_bytes`, `borrow`/`borrow_mut`/`end_borrow`, `use`, `drop`, `collect`, `close`, nested `no_gc`. The conversion lane is independent; do not assume its new ops exist. Source scripts must remain below fixed statement, region, byte, and step bounds.
- Every generated case logs seed and the exact `.cg` source on failure. A fixed seed corpus is checked in or explicitly enumerated in tests. No nondeterministic timing assertions; compare collection counts, byte/failure invariants, validity, and zero collection inside guarded paths.
- Invalid cases assert checker rejection for use-after-move, borrow conflict, close with live handle, and collection under a guard. Distinguish static error from runtime `AllocationError`; a failed allocation must not emit a successful report.
- Raw pointer misuse is not expressible in safe source. Do not claim zero memory bugs solely from this corpus; audit/fuzz coverage remains a release gate.

## Review focus

1. At least 100 deterministic valid generated cases pass from fixed seeds, with varied auto/manual modes, two-region isolation, owned moves, and nested guards.
2. Deliberate one-operation mutations of valid scripts produce expected static diagnostics; unknown/new diagnostics are not silently ignored.
3. Report counters obey `live <= committed <= peak`, closed region live/committed zero, manual collection count exactly explicit requests, and no_gc collection delta zero on success and failure.
4. Repeating a seed reproduces identical source and expected model result. The generator never loops forever or allocates unbounded memory.
5. A model/implementation mismatch prints source, seed, expected, actual, and report snapshot without hiding failure under a broad catch.

## Tasks

### 1. Reference state and generator

- [ ] Add red tests for seeded determinism, bounded generation, and model counter transitions.
- [ ] Implement state transitions and source emitter. Use stable names/byte sizes so generated scripts fit the current parser and runtime limits.
- [ ] Run fixed seeds and make the test failure output reproducible.

### 2. Source/runtime conformance

- [ ] Add cross-layer tests for generated valid scripts and exact invalid fixtures. Validate static diagnostic codes, dynamic allocation errors, zero collection under guards, and per-region isolation.
- [ ] Add at least one failure path with a nested `no_gc` budget and a previous successful allocation to prove no rollback claim and zero collection. If the public API hides the report on failure, use a small direct-runtime companion test for the counter assertion and label the layer.
- [ ] Save a compact machine-readable corpus manifest with seeds and model version; no synthetic pass records.

### 3. Verification and handoff

- [ ] Run `cargo test -p cgx2-memory-conformance --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-memory-conformance --all-targets --offline -- -D warnings`.
- [ ] Document replay command, model limits, and unresolved memory safety gates. Commit only owned paths.

**Acceptance:** Fixed seeds repeatedly exercise the source-to-runtime memory path and expose counter/ownership regressions with reproducible programs; this remains finite evidence rather than a proof.
