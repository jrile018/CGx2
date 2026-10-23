# CGx2 executable release guide and acceptance map

ID: S1-12. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Build an executable developer/agent guide and acceptance inventory for the complete first-release contract, with runnable fixtures checked against a supplied pinned compiler.

## Exclusive write ownership

- `docs/first-release/**`
- `tools/docs-check/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Cover language/packages, memory, effects/capabilities, contracts/proofs, CLI, notebooks, explicit Python versus direct LibTorch, source targets and their current limits.
2. Put runnable examples beside machine-readable expectations; a small checker invokes a caller-supplied compiler and reports command, version/hash, outputs/errors and package-relative diagnostics.
3. Separate implemented, expected-unsupported and future contract examples. An absent compiler/foreign runtime is blocked; a future example is not passed because it is skipped.
4. Include outside-checkout installation smoke instructions and a complete command coverage inventory. Keep shared design docs, root README, roadmap/progress and coordinator plan files read-only.
5. Use only common-base features for presently runnable examples; reserve named fixtures for S2/S3 features without guessing evolving syntax.

## Acceptance and checks

Run checker against supplied reviewed cgx2 binary; positive/negative fixtures and exact unsupported behavior; malformed expectation files; tampered compiler identity; absent prerequisites; deterministic report and safe output paths.

Run the Python/tool tests relevant to the owned deliverable and its actual executable acceptance path. Do not write fixtures into other lanes.

## Commit and stop

Usable guide, executable checker and requirement-to-fixture map to rerun at release integration.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
