# Memory Assurance Audit Implementation Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Work directly in this chat. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Turn the Wave 8 direct-runtime state machine into broader, reproducible evidence for the memory release gate and repair any demonstrated runtime defect.

**Architecture:** Keep `cgx2-runtime::Session` public signatures fixed. Extend the independent conformance model with adversarial traces and coverage counters; add targeted regression tests for any mismatch before fixing it. Record what the finite test cannot establish in a machine-readable audit report and crate README.

**Tech stack:** Rust 1.98, safe Rust test generator, existing v1/v2/v3 conformance models; offline Cargo. Run Miri or sanitizers only if already installed.

**Spec:** `docs/validation-plan.md` memory hard gates; `crates/cgx2-runtime/README.md`; `plans/wave9/README.md`.

## Global constraints

- Own only `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**`, and this plan. Keep public `Session` signatures, collection modes, `no_gc` accounting, error families, and source-visible report fields stable. Send an interface discrepancy to the coordinator in the handoff; do not quietly change the contract.
- Retain exact replay for v1 (128 seeds), v2 (256 seeds), and v3 (1,000 seeds) and stable `--seed N --model-version 3`. Add a version 4 if PRNG, operation set, transition model, or serialization changes. Bound each trace to <=128 operations, <=4 regions, <=128 retained handles, <=64-byte payload, and two nested guards. Keep a fixed CI set of at least 2,000 v4 seeds and a documented extended 10,000-seed command. Runtime and wall-clock limits must be recorded.
- Check results and deterministic counters after every operation. Specifically target stale handles after close/recreate, roots through links, cross-region links, owned move failure preserving owner, independent copies, near-limit zero-length allocations, nested guard exhaustion, auto safepoints after guard exit, and wrong-session handles. Never treat memory foreign to CGx2 as charged to a region.
- A passing finite corpus is evidence, not a proof or a complete hard gate. Do not claim thread safety, arbitrary object layout safety, or PyTorch memory coverage.

## Review focus

- Shrinking a failure must preserve the mismatch and produce a replayable trace.
- Guard failures must leave committed/live bytes unchanged and collect nowhere.
- Closing one region must not invalidate live handles in another.
- Seed generation must be byte-for-byte deterministic across two runs of the same model version.
- An audit report must show skipped operations and untested state dimensions, not only pass counts.

## Tasks

### Task 1: Corpus and coverage accounting

**Files:** `crates/cgx2-memory-conformance/src/v4.rs`, `src/main.rs`, `corpus/seeds-v4.csv`, `tests/v4.rs`.

- [ ] Write failing tests for exact v4 seed manifest, generator determinism, bounded trace sizes, counters for each required operation/error family, replay and shrink determinism.
- [ ] Implement v4 generator/model and per-step assertions; preserve v1–v3 behavior. Fail the test if an intended adversarial family has zero coverage.
- [ ] Run the fixed 2,000 seeds twice and compare deterministic summaries; run the extended 10,000 set once outside CI and record seed count, time, and observed failures.

### Task 2: Targeted runtime audit

**Files:** `crates/cgx2-runtime/src/**`, `crates/cgx2-runtime/tests/**`, conformance regression fixtures.

- [ ] Review public unsafe-free runtime paths for invalid-handle, `checked_add`, region close, collection, guard unwind and owner transfer errors. Add tests for concrete uncovered paths; avoid tests that merely mirror implementation.
- [ ] For each real mismatch, save original and minimized traces, add a failing regression, make the smallest runtime repair, and rerun v1–v4. If none appears, state that plainly.
- [ ] Run Miri/sanitizers only when present; record unavailable tools as not run.

### Task 3: Evidence and handoff

- [ ] Add a versioned audit summary under `crates/cgx2-memory-conformance/` listing seed counts, operation coverage, fixed defects, exact commands, CPU/runtime, and open hard gates.
- [ ] Run `cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline`, fmt, and targeted Clippy `-D warnings`; commit only owned paths and report exact results.

**Acceptance:** A reproducible expanded corpus and audit cover the named failure modes, with replayable minimal defects and no claim that finite fuzzing certifies the language.
