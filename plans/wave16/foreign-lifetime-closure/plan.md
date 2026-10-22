# Foreign lifetime and closure evidence

**Goal.** Establish what the direct native PyTorch route really guarantees after Wave 15's preloaded-module check and bounded tensor copy. Do not route PyTorch through Python.

**Ownership.** Only `crates/cgx2-bind/**` and `crates/cgx2-libtorch/**`. Keep `Torch::open_pinned(root, binding_id)` and source-facing APIs compatible. Do not edit the CLI, package checker, root Cargo files, shared docs, or other lanes.

## Deliverable

1. Audit closure identity across preflight, `dlopen`, tensor operations, and release. Test executable mapped files with arbitrary names, symlinks/inode changes, removed or replaced files, altered `DT_NEEDED`, and a mismatched already loaded module. Reject unverifiable state before calling foreign code when possible. State remaining time-of-check/use and malicious native-code limits plainly.
2. Test tensor handle ownership and bounded `to_vec` under creation, alias/copy, error, teardown, double release, and oversized or inconsistent foreign shape reports. Keep host/device allocations labeled foreign, outside CGx2 collection accounting and `no_gc` guarantees. Do not assume the facade obeys the declared ABI merely because it is hashed.
3. If a compatible real LibTorch CPU artifact exists, run pinned create/add/copy/release against it and check no Python runtime loaded. If absent, keep mock tests and mark real compatibility unrun with detection command and version requirements.

## Verification and handoff

Run offline tests of both crates, formatter, targeted Clippy with warnings denied, and adversarial fixture commands. Report exact trust boundary, compatibility status, and commit ID. Stop for review.
