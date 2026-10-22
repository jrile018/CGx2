# CGx2 Memory Assurance Plan

> Implement this plan in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Expand the deterministic memory model from the frozen Wave 5 profile through Wave 6 owned/managed conversions and stress the highest-risk failure paths. This adds evidence for the first-release memory gate; it cannot prove absence of all memory bugs.

**Architecture:** Extend `cgx2-memory-conformance` with a versioned state machine for `memory.promote_owned(region, owned)` and `memory.copy_to_owned(managed)`. Generate valid bounded scripts and deliberate invalid mutations with reproducible seeds. Compare source execution reports and, where source errors hide reports, direct `cgx2-runtime::Session` counters. Add targeted runtime tests for atomic allocation failure, roots, cross-region links/close, nested guards, and stale handles. Fix confirmed runtime defects within this lane while preserving public APIs.

**Tech stack:** Rust 1.98; existing CLI/runtime/conformance crates; no new unsafe code or external fuzz dependency.

**Specification:** `docs/language-design.md` memory model, `docs/validation-plan.md` hard gates, `crates/cgx2-memory-conformance/README.md`, `plans/wave7/README.md`.

## Owned paths and boundaries

- Own only `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**`, and this plan. Do not modify syntax, sema, ownership, CLI, root files, or other lanes. Preserve `Session` method signatures and existing error codes. If a source checker defect blocks a case, save the minimal reproducer and report it; do not silently drop the case.
- Keep fixed corpus v1 for historical replay. Add a v2 manifest with at least 256 fixed seeds and documented PRNG/version. Generated source includes both conversion intrinsics, two regions, explicit root/drop/collect, auto/manual behavior, moves/borrows, nested `no_gc`, and failed allocations within defined size/step bounds. Define expected counters from the independent model rather than by copying runtime output.
- Verify ownership of failed promotion: the original owned bytes remain available, no managed handle is fabricated, and collection does not occur under `no_gc`. A successful promotion consumes one owner; copying creates distinct storage and leaves the managed source valid. Explicitly test zero-length payload and near-limit byte accounting.
- Inject invalid operations with exact diagnostic families and source spans: use after promotion, borrow conflict, close with live handle, stale handle after collect, cross-region link rejection, and collect within `no_gc`. Distinguish checker rejection from dynamic `AllocationError` and document any case unexpressible in safe source.
- `Session` is a single-threaded byte-object runtime; no concurrency, arbitrary object layout, foreign memory, sanitizer certification, or universal safety proof is claimed. Report the tested state space and remaining audit/fuzz gaps.

## Tasks

### Model and generated corpus

- [ ] Write red tests for v2 generator determinism, source bounds, model conversion transitions, and a deliberately mismatched report.
- [ ] Implement versioned v2 generation, expected report checks, and replay `--seed N --model-version 2`; keep v1 replay stable.
- [ ] Run all fixed v1 and v2 seeds twice and verify byte-identical generated scripts/results.

### Failure and isolation checks

- [ ] Add conversion success/failure tests at source and runtime layers. On failure, assert no successful report is printed and inspect preserved state with direct runtime tests.
- [ ] Add per-region isolation, stale-handle, nested-guard, region-limit, and zero-byte cases with exact counters and no hidden collection.
- [ ] If a runtime defect is found, add a regression test first, repair it without public API breakage, and record the exact semantic change.

### Verification and handoff

- [ ] Run `cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-runtime -p cgx2-memory-conformance --all-targets --offline -- -D warnings`.
- [ ] Commit only owned paths; report seed counts, replay command, any defects, and unresolved memory release gates.

**Acceptance:** A reproducible conversion-aware corpus and targeted runtime tests detect report, ownership, budget, and region-isolation regressions without overstating finite-test evidence.
