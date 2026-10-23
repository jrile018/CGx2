# CGx2 native binding SDK and direct inference

ID: S1-04. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Implement a useful bounded native binding SDK and direct CPU inference facade: variable-length UTF-8/byte results, reviewed foreign handle lifetimes, and a pinned tensor inference sequence.

## Exclusive write ownership

- `crates/cgx2-bind/**`
- `crates/cgx2-ffi-native/**`
- `crates/cgx2-libtorch/**`
- `crates/cgx2-ffi-core/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Own the additive shared ForeignValue/error/declaration implementation under cgx2-ffi-core, following contracts.md. Preserve existing scalar/native and tensor v1-v3 APIs and locks. Add explicit versions for new layouts/symbols.
2. Support bounded variable-length copied UTF-8/byte outputs, checked ownership/release functions, and owned/shared handle contracts. Validate ABI/hash/export/layout before invocation; prevent duplicate handle release and foreign unwinding.
3. Extend direct tensor creation/shape/dtype/device inspection and selected arithmetic/activation/matrix operations sufficient for one real CPU inference pipeline. Autograd is a separately reported extension only if it fits after required inference acceptance.
4. Provide offline package/facade examples and real pinned numerical/lifetime/no-libpython acceptance. Do not modify `.cg` compiler integration or invent real-library evidence. Keep new cgx2-ffi-core dependency-free if possible; coordinator owns Cargo.lock reconciliation.

## Acceptance and checks

Wrong identity/ABI/symbol/layout; bounded output and invalid UTF-8; exception/panic cleanup; release once; retained buffers; real CPU reference equality when matching LibTorch available. Existing facade regressions.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-bind`, `cgx2-ffi-native`, `cgx2-libtorch`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

Public additive SDK and exact tensor ABI/lock specification, S2 source integration recipe, real versus fixture-only evidence table.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
