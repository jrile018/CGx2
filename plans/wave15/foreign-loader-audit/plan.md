# Foreign loader and tensor lifetime audit

Use the attached worktree; read shared Wave 15 references and this plan. Commit only owned paths and stop for review. This route is a direct native C++ LibTorch facade; no Python bridge or fallback.

**Goal.** Wave 14 pins a facade's ELF closure but checks mapped modules after `dlopen`. Reject known substituted/preloaded modules before loading, bound foreign-reported lengths before host allocation, and make the remaining malicious-library threat explicit. Preserve `Torch::open_pinned(root, binding_id)` for the source lane.

**Ownership.** Only `crates/cgx2-bind/**` and `crates/cgx2-libtorch/**`. Do not edit CLI, package checker, notebook, root Cargo files, shared docs or plans. New APIs must be additive.

## Deliverable

1. Before `dlopen`, inspect mapped-module identity for all pinned SONAMEs/basenames and reject a conflicting mapped inode/path or an unpinned LibTorch module. Verify expected `DT_NEEDED` closure and loader search path policy. Recheck after load and before each foreign operation if the guarantee depends on modules remaining fixed; otherwise return an explicit unverified state. Bound inspection output and reject ambiguous/proc-unavailable cases.
2. Provide a bounded `Tensor` copy path: accept an expected maximum length from the caller, query the facade's reported shape, reject a length beyond the cap or mismatch before allocating output storage, and preserve release-on-error. Keep the existing `to_vec` API only with a documented safe bound or deprecate it without breaking current callers.
3. Add mock fixtures for preloaded evil dependency, modified library after preflight, excess/missing `DT_NEEDED`, oversized reported length, cross-adapter handle and double release. Demonstrate that a preloaded conflict is rejected before any newly loaded facade constructor; do not claim protection from every race or hostile native code. Record foreign allocation outside CGx2 GC accounting and `no_gc`.
4. Run a real compatible LibTorch CPU add/copy/release test when an artifact and exact dependencies are installed. If absent, keep it conditional and report unrun. Explicitly state that C ABI/lifetime trust still requires reviewed native binaries and that post-`dlopen` rejection may occur after constructor execution.

## Verification

Run offline tests for both owned crates, format, targeted Clippy with warnings denied, mock substitution and length tests, and conditional real test. Report module paths/inodes actually observed, failure timing, remaining threat model and commit SHA.

**Acceptance.** Known preloaded substitution is rejected before loading, unbounded foreign lengths cannot allocate arbitrary host memory, and unverifiable loader states fail closed rather than appearing pinned.
