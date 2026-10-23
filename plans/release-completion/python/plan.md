# CGx2 pinned Python package runtime

ID: S1-05. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Add explicit pinned Python package invocation with copied typed values and environment identity validation, preserving existing scalar CLI behavior and native/Python separation.

## Exclusive write ownership

- `crates/cgx2-foreign/**`
- `crates/cgx2-python-bridge/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Implement an additive package-environment API with explicit interpreter, package/dependency lock, allowed callable signatures and expected results. Validate identities before imports can run package initialization.
2. Use existing embedded CPython support for the package route where appropriate; document embedded versus child-process guarantees. Preserve the legacy one-shot scalar child interface and `-I -S` behavior.
3. Support bounded copied strings/containers/arrays that existing APIs can represent, defined exception translation, and deterministic release of Python references. No implicit LibTorch-to-Python fallback.
4. Use local fixture packages for deterministic tests and provide a real python-dateutil runner requiring a supplied pinned environment. Do not install packages automatically. Keep existing types compatible; shared wire schema is documentation-only until S2 integration.

## Acceptance and checks

Wrong interpreter/env/hash/module/signature/result; import side effects only after preflight; exceptions and reference cleanup; hostile serialization/oversized results; existing scalar CLI boundary read-only; actual dateutil test only when prerequisites supplied.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-foreign`, `cgx2-python-bridge`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

Pinned environment manifest/example, callable API, capability/effect requirements and S2 typed-source integration instructions.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
