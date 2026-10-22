# Pin and harden the direct LibTorch CPU adapter

> Use the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Make the existing direct native CPU `float64` tensor facade and its offline binding declaration precise enough to integrate into `.cg` later, without using Python as a middleman or claiming real LibTorch conformance on a host lacking a compatible artifact.

**Architecture:** Reconcile `cgx2-bind`'s `libtorch` route with the actual version-pinned C++ facade in `cgx2-libtorch`. The lock must name the exact facade artifact hash, target OS/arch, C ABI boundary, torch version assertion, exported operations and handle release semantics. Call the ABI by its real shape; do not describe the C++ facade as LibTorch's stable C ABI if it uses `torch/torch.h`. Static `bind check` must inspect/pin bytes without `dlopen`; the actual adapter performs runtime identity/version checks before a tensor call. Python libraries and `libpython` may not be linked or loaded by this route.

**Ownership:** Only `crates/cgx2-libtorch/**` and `crates/cgx2-bind/**`. Read package/source native C code but do not edit it, root manifests/lock, docs or plans. Do not require new APIs from the parallel source-memory lane. A crate-local dependency change that affects root Cargo.lock must be reported for coordinator update.

## Required behavior

1. Add a mock C facade fixture for success and negative tests: wrong artifact hash, missing export, wrong version/ABI, invalid dtype/shape/device, mismatched lengths, foreign error, double release, handle created by another adapter, and output-copy bounds. Distinguish mock-facade contract checks from real LibTorch tests in names and reports.
2. Preserve explicit LibTorch ownership of tensor/device allocations. CGx2 owns only wrapper handles and copied host buffers; no claim that `no_gc` controls foreign memory. Verify RAII and error cleanup, and ensure failed operations cannot publish an uninitialized tensor or values. Keep CPU-only `float64` one-dimensional scope unless a separately tested extension is truly implemented.
3. Maintain a conditional real test requiring `CGX2_LIBTORCH_FACADE` and exact `CGX2_LIBTORCH_VERSION`, with visible skip when absent. With a compatible artifact, verify create/add/shape/copy/release and inspect dependency loading for `libpython`; do not claim this test ran if the artifact is unavailable.
4. Update owned READMEs with version/hash trust boundaries, supported facade ABI, native test commands, copied bytes/handle rules and precise `.cg` integration gap.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-libtorch -p cgx2-bind --offline`, format and targeted Clippy `-D warnings`, plus the mock facade fixture. Run the conditional real test only when the required artifact exists. Commit owned paths and report SHA, exact artifact availability and remaining source/GPU limits.

**Acceptance:** A pinned, source-independent LibTorch facade contract has adversarial native tests and an honest real-library gate, ready for later typed `.cg` tensor integration.
