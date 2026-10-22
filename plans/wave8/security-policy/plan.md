# CGx2 Advisory Security Policy Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Give developers and coding agents a source-located warning when a `.cg` file violates an explicit external effect policy or follows a small documented taint path. This is a bounded security analyzer, not source capability enforcement or a vulnerability proof.

**Architecture:** Extend `cgx2-effect-index` to classify Wave 6 owned/managed conversion intrinsics accurately and expose stable source-located call/effect facts. New `cgx2-security` reads a versioned JSON policy and one `.cg` file, uses the effect index plus a small AST-based dataflow for direct local bindings/calls, and emits deterministic text/JSON/SARIF 2.1.0 findings. Unknown/unresolved/imported calls fail closed under the default policy. No runtime behavior changes.

**Tech stack:** Rust 1.98, existing syntax/effect-index crates, offline JSON dependency, no external security service.

**Specification:** `docs/language-design.md` capabilities/security, `docs/validation-plan.md` security suite, `crates/cgx2-effect-index/README.md`, `plans/wave8/README.md`.

## Owned paths and limits

- Own only `crates/cgx2-effect-index/**`, new `crates/cgx2-security/**`, and this plan. Do not edit parser, sema, package, CLI, runtime, root files, or other lanes. Preserve existing effect-index command/schema or bump schema explicitly with compatibility tests.
- Policy JSON v1 names allowed effects and `allow_unknown` (default `false`). Unknown calls and imported functions without sound body effects must not be labeled pure. Classify `memory.promote_owned` as allocation/mutation and `memory.copy_to_owned` as allocation; retain existing `no_gc`/collect rules. An allowed effect is not a runtime capability token.
- Define one limited taint rule: values from `arg(...)` are untrusted; direct immutable/mutable binding and scalar expression propagation preserve taint; passing a tainted value to `python.call` or `ffi.call` (or an explicitly documented sink) produces a finding. Propagate through local direct calls only when a bounded summary can be proven; otherwise mark `Unknown` rather than claiming safe. Do not infer sanitization from arbitrary function names. Taint is advisory and syntax-only when foreign calls are outside current sema.
- Findings include stable rule ID, severity, message, source byte span, and related source/sink location when known. SARIF is valid 2.1.0 with UTF-8 byte-to-line/column conversion, deterministic ordering, and no leakage of full source by default. Bound input/policy size and graph iteration. Invalid policy or parse failure is distinct from a clean audit.
- Document seeded false-positive/missed cases. This does not prove general injection safety, enforce source capabilities, or analyze Python/PyTorch internals. A `cgx2-security` zero-finding result cannot clear the first-release security gate alone.

## Tasks

### Effect policy

- [ ] Add red tests for Wave 6 conversion effects, direct/transitive disallowed effects, unknown/imported call failure, allowed effects, malformed policy, deterministic JSON and exit status.
- [ ] Implement the policy command `cgx2-security audit FILE.cg --policy POLICY.json --format text|json|sarif`, with source spans and stable rule IDs.

### Taint and SARIF

- [ ] Add positive/negative fixtures for tainted `arg` flowing to a documented foreign sink, local binding propagation, a clean literal, aliasing/unsupported path marked Unknown, and UTF-8 span conversion.
- [ ] Emit valid SARIF 2.1.0 with tests that parse its JSON and inspect rule/result locations. Include README examples and an explicit limitation table.

### Verification and handoff

- [ ] Run `cargo test -p cgx2-effect-index -p cgx2-security --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-effect-index -p cgx2-security --all-targets --offline -- -D warnings`.
- [ ] Commit only owned paths; report exact pattern coverage, false positives and unresolved capability enforcement.

**Acceptance:** Policy and selected taint violations produce deterministic source-located findings, unknown calls are conservative, and the analyzer's limits are explicit.
