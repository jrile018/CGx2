# Direct CPU float64 matrix route

**Goal.** Exercise useful PyTorch compute through LibTorch's C++ API with no Python interpreter in the runtime call path. Keep the Wave 18 pinned one-dimensional route source- and binary-compatible.

**Ownership.** Edit only `crates/cgx2-bind/**` and `crates/cgx2-libtorch/**`. Do not edit `.cg` parser/checker/CLI, notebook, root Cargo files, or shared docs.

## Deliverable

1. Add a versioned, additive facade contract for contiguous row-major CPU `float64` matrices: checked construction from `(rows, cols, input)`, matrix multiplication, shape query, exact-capacity bounded copy, and explicit handle release. Bound dimensions and `rows * cols * 8` with checked arithmetic before calling C++; reject zero, excessive, mismatched, or overflowed shapes with stable errors. Preserve v1 vector symbols and locks so existing packages still open.
2. Extend the adapter and binding preflight to pin exact facade symbols/ABI for the new route. Reject wrong ABI/version, an omitted new symbol, stale artifact bytes/inode, ambiguous `DT_NEEDED`, and unsafe search paths. Handles must have clear owned/borrowed lifetimes, including partial error returns, aliases, failed copy, and double release. Label CGx2 copies versus foreign LibTorch allocations accurately.
3. Build an exact local copied package against the available CPU Torch 2.11 C++ libraries if they remain installed. Run at least non-square multiplication and shape/copy/release, compare numeric outputs with a hand-calculated reference, and check `/proc/self/maps` for no mapped `libpython`. Record command, compiler/ABI, package artifact hashes, and result. If unavailable, mark real test unrun; mocks cannot substitute.
4. Update only owned docs. This is a direct C++ LibTorch matrix adapter, not arbitrary Python-library access or a general `.cg` tensor value. Do not add GPU claims.

## Verification and handoff

Run offline `cgx2-bind` and `cgx2-libtorch` suites, v1 regression tests, new mock negative fixtures, real ignored CPU test if available, format, and Clippy with `-D warnings`. Commit only owned source/tests/docs, never local binaries or host-absolute lock files. Report SHA, exact files, mock versus real results, artifact inventory, and remaining ABI/lifetime limits; stop for review.
