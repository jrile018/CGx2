# CGx2 Runtime State-Machine Fuzz Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Search for memory safety and accounting bugs beyond fixed source scripts using reproducible randomized `Session` operations, targeted invalid handles, and a shrinking/replay workflow. Treat every observed defect as a release blocker until fixed and retested.

**Architecture:** Extend `cgx2-memory-conformance` with an independent direct-runtime state machine that generates bounded sequences of region creation, owned allocation/promotion/copy, managed allocation/root/unroot/link/read, collection, close, and nested `no_gc`. Keep a model of valid handles, region membership, owner state, live/committed/peak bytes and expected error families. Execute sequences against `cgx2-runtime::Session`, check invariants after every step, and print the seed and minimal failing sequence. Add regression tests for any found defect in `cgx2-runtime` without changing public API signatures.

**Tech stack:** Rust 1.98, existing runtime/conformance crates, deterministic in-tree PRNG, optional Miri if already installed; no network fuzz service or unsafe code.

**Specification:** `docs/validation-plan.md` memory hard gates, Wave 7 v2 corpus, `crates/cgx2-runtime/README.md`, `plans/wave8/README.md`.

## Owned paths and limits

- Own only `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**`, and this plan. Do not edit source syntax/sema/CLI, root manifests/lock, or other lanes. Preserve `Session` public signatures and collection semantics. If a public interface issue appears, report the smallest reproducer for coordinator review instead of changing the contract silently.
- Use a fixed manifest of at least 1,000 seeds with bounded steps (for example <=128), objects, regions, payload bytes and wall time. Record PRNG and model versions. A `--seed N --model-version 3` command replays the exact sequence. Shrink by deleting operations and simplifying byte sizes while preserving the mismatch; retain the original and minimized seeds/trace.
- Check after every operation: `live <= committed <= peak`; manual collection count changes only on explicit collect; `no_gc` never collects even on failures; region limits and guard budgets fail before commitment; stale/foreign/cross-region handles return defined errors; successful promotion consumes the owner and failure returns it; copy makes independent bytes; collecting/closing one region does not change live handles in another. Check exact counters where model semantics permit, and distinguish performance-only pause values.
- Use safe Rust APIs for the generator. This finite fuzz run is not proof of universal safety, sanitizer coverage, concurrency safety, or foreign-memory isolation. Document the state space, skipped operations, and whether Miri ran. Do not mark the hard release gate passed solely from 1,000 seeds.

## Tasks

### Model and replay

- [ ] Add red tests for deterministic generation, bounded sequences, a deliberately faulty model/trace, and shrinker reproducibility.
- [ ] Implement v3 step generator, independent transitions, per-step invariant checker, seed manifest and replay CLI. Preserve Wave 5 v1 and Wave 7 v2 corpus behavior.
- [ ] Run the fixed seed set twice and compare deterministic results; record exact seed/step on failure.

### Fault and regression checks

- [ ] Include targeted stale handle, wrong-session handle, cross-region link, nested guard exhaustion, zero-length and near-limit allocation sequences. Assert errors and counters at the right layer.
- [ ] If a real bug appears, add its minimized regression first, repair only within owned runtime files, rerun all three model versions, and disclose the failure/fix.
- [ ] Run `cargo miri test -p cgx2-runtime` only if Miri is already available; report unavailable explicitly rather than treating it as a pass.

### Verification and handoff

- [ ] Run `cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-runtime -p cgx2-memory-conformance --all-targets --offline -- -D warnings`.
- [ ] Commit only owned paths; report seed count, runtime, model coverage, bugs, and remaining hard memory gates.

**Acceptance:** A reproducible bounded state-machine fuzz corpus checks memory invariants after each operation and makes any mismatch easy to replay and minimize without overstating its assurance.
