# Explicit source-level native C fixture calls

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Let a checked `.cg` package make one small, explicit native C ABI call without Python. Establish the source-to-package-to-runtime boundary for reviewed facades; do not advertise arbitrary native symbols as safe or claim LibTorch/PyTorch support yet.

**Architecture:** Define one exact scalar call shape matching the existing `cgx2-ffi-native` fixed C fixture (status return, `i64` argument, `i64*` out, no callback or retained pointer). Align `cgx2-bind` lock shape, preflight, and `cgx2-ffi-native` declaration so a shape cannot pass one component and mean another in the next. The `.cg` syntax must mark the call `unsafe foreign` (or an equally explicit reviewed escape hatch) and require declared `foreign` effect and `native` capability. A schema 2 package resolves a binding ID from its confined lock. Before a run, validate the exact artifact hash, host platform, identity/version contract and declared symbols; the actual call uses only this fixed adapter shape. The lock is a byte pin and assertion, not proof of the physical C signature. Never let a user-authored lock turn arbitrary code into an ordinary safe call.

**Ownership:** Only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**`, `crates/cgx2-package/**`, `crates/cgx2-bind/**`, and `crates/cgx2-ffi-native/**`. Do not edit emitters, runtime, notebook, LibTorch, root manifests/lock, docs or plans. Consume Wave 11 APIs only. If a new Cargo dependency would require the root lock, prefer existing dependencies and report the coordinator-only lock update needed.

## Required behavior

1. First write a short owned-crate design note and fixtures that state exact source grammar, target ABI, ownership, error mapping, effect/capability rules, version/hash checks and trust status. Include a compiled C fixture and source `check/run` cases. Reject a falsely safe declaration, undeclared effect/capability, missing/wrong binding ID, bad hash/version/platform, missing symbol, unsupported signature, stale package input and an FFI error status before any success value is emitted.
2. A `.cg` source call must use the package's pinned binding and run through the reviewed fixed adapter, not CPython. API index and JSON diagnostics expose the foreign boundary, capabilities, binding ID, and source span. Standalone scripts without a manifest cannot silently search host libraries. Repeated calls must not leak or retain borrowed input/output memory. No callback, tensor, GPU or general byte-pointer signature is in this profile.
3. Preserve existing scalar/Python-only paths and v1 packages. `cgx2 check` and `cgx2 run` must agree; native `cgx2c` and Python emission must reject this new source form clearly until their own supported profile includes it. If the frozen emitter cannot detect the form safely, narrow or defer source syntax and report the dependency instead of allowing silent translation.
4. Document what is runtime checked versus unsafe caller responsibility. Use the exact C fixture's true signature in tests; a malicious library can lie about identity/signature, so no general memory-safety claim follows from pinning.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-syntax -p cgx2-sema -p cgx2-cli -p cgx2-api-index -p cgx2-package -p cgx2-bind -p cgx2-ffi-native --offline`, format, and targeted Clippy `-D warnings`. Run direct CLI `check/run` success and rejection fixtures. Commit owned paths and report SHA, exact syntax, ABI, trust boundary, diagnostic samples and limits.

**Acceptance:** One fixed-shape pinned C call works from `.cg` with explicit unsafe/effects/capability semantics and no Python runtime; an arbitrary declaration remains unable to claim safe physical ABI compatibility.
