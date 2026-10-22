# Memory runtime assurance v5

> Work in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Expand adversarial evidence for the current `Session` byte-object/region/owned API, especially failure, cancellation, stale-handle, root, and `no_gc` paths, and repair any demonstrated defects.

**Architecture:** Extend the independent state-machine model and deterministic seed corpus instead of merely repeating the implementation. Include operations added or corrected in Wave 10's failure-path work. Use bounded fault injection or allocation seams only where a real failure branch cannot otherwise be reached. Keep the public `Session` API and report schema stable so the source and native lanes can consume the Wave 10 contract.

**Tech stack:** Rust 1.98; `cgx2-runtime`, `cgx2-memory-conformance`; optional Miri/sanitizer when installed, never a fake pass.

**Spec:** `docs/language-design.md` memory semantics; `docs/validation-plan.md` hard memory and foreign boundary gates.

## Constraints

- Own only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Read but do not edit source/compiler, FFI, root manifests/lock, docs/plans. No public method signature or error variant changes that parallel lanes would need to consume.
- Generate operations that combine auto threshold collection, insufficient headroom with reclaimable/unreclaimable objects, intrinsic impossible requests, manual regions, nested `no_gc`, owned/managed copies, failed promotions, links/cycles, stale generations, region close, and panic/unwind of guards. For each step compare results, live/committed/peak/failure/collection counters, surviving payloads and handle validity against an independent model.
- If a defect appears, minimize it to a fixed seed and a focused regression before repair. Distinguish allowed auto collection performed to make an allocation possible from forbidden collection in `no_gc`, manual, or intrinsically impossible requests. Never label finite fuzzing a proof or claim coverage of typed objects, foreign allocators or threads.
- Produce a machine-readable v5 audit report with exact seeds, operations, errors, skips, repeated-run agreement, and optional dynamic-check tool version/result. Preserve v4 corpus and prior tests.

## Tasks and checks

1. Add failing or new transition tests for Wave 10 preflight/reclaim behavior in the independent model, then extend seed corpus and reducer/replay command. Keep CI runtime bounded; provide an extended opt-in run.
2. Run repeated deterministic corpus and targeted `cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline`. Run available Miri/sanitizer only if configured; record unavailable checks honestly. Repair only reproducible defects with focused tests.
3. Run format and targeted Clippy `-D warnings`; update owned README with v5 coverage and limits. Commit owned paths and report seed counts, defects or none, checks, open safety gates and SHA.

**Acceptance:** v5 exercises the new failure/collection transitions with reproducible state comparisons and no observed violation of the existing memory contract.
