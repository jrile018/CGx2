# LibTorch dependency closure and loader identity

Use the attached worktree; read shared Wave 14 references and this plan. Commit only owned paths and stop for review.

**Goal.** The reviewed facade hash currently pins only one `.so`; dynamically loaded LibTorch dependencies can vary. Pin the actual local dependency closure and reject an ambiguous or substituted loader path before claiming the route is pinned. Preserve the frozen `Torch` adapter API consumed by the source lane.

**Ownership.** Only `crates/cgx2-libtorch/**` and `crates/cgx2-bind/**`. Do not edit source checker, CLI, root workspace manifest/lock, shared docs, plans, or benchmark. The lane may add owned fixtures and README material. If an OS loader limitation blocks a guarantee, fail closed and document it.

## Deliverable

1. Define a versioned lock entry for a facade and each required transitive native library, including canonical absolute path, SHA-256, OS/architecture, and expected soname or identity where available. Reject symlinks/path traversal and duplicate logical names with different bytes. Bound closure size, file size, and path length.
2. Verify closure before opening the facade and again against the actually resolved loaded modules if the platform exposes reliable loaded-path evidence. Detect missing, extra, mismatched, and search-path substituted LibTorch libraries. Do not rely on the compile-time `Torch_VERSION` string alone. If full loader observation is unavailable, return a distinct unverified state and do not call it fully pinned.
3. Add mock native shared-library fixtures for good closure, altered transitive dependency, wrong version, and a loader-search-path substitution. Keep C ABI/handle lifetimes unchanged. Make tests reproducible offline and Linux-scoped where appropriate.
4. Update owned README with lock schema, preflight/open sequence, what a hash proves, real artifact setup, and what still requires ABI/lifetime validation.

## Verification

Run `cargo test -p cgx2-bind -p cgx2-libtorch --offline`, format, targeted Clippy with warnings denied, and a conditional real-LibTorch test only when an exact compatible installation exists. Report each loaded dependency observed or explicitly report why it could not be observed. Give commit SHA.

**Acceptance.** A changed dependency or loader substitution cannot be reported as a pinned success. Mock fixtures establish rejection behavior; a release claim still requires an actual compatible LibTorch run.
