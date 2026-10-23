# CGx2 release core commands

ID: S1-01. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Deliver the installed native build, emission and public API command surface: `cgx2 build PATH`, `cgx2 emit PATH --target rust|python`, and `cgx2 api --json PATH` for single files and checked local packages. Reuse the current compiler and emitter APIs; keep current commands compatible.

## Exclusive write ownership

- `crates/cgx2-cli/**`
- `crates/cgx2c/**`
- `crates/cgx2-emit-rust/**`
- `crates/cgx2-emit-python/**`
- `crates/cgx2-api-index/**`
- `crates/cgx2-package/**`
- `crates/cgx2-sema/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Expose checked build/emission through the product CLI with deterministic outputs, explicit destination handling, target rejection before writing, atomic output replacement, package-relative diagnostics and correct exit codes.
2. Exercise the installed binary from a directory outside the checkout, with no repository-relative executable assumption. Preserve Python-free native invocation and explicit Python worker routing.
3. Provide a compiler-owned acceptance fixture for a checked owned-input map/zip pipeline and document the exact existing execution-policy entry point to be wired in S2. This is a consumer contract/fixture, not a request to implement automatic GPU selection.
4. Do not alter AST/type/value variants or existing imported-summary APIs. Variants, generics, full memory/foreign lowering and specialist command integration are scheduled in S2.

## Acceptance and checks

Single-file and multi-module package positive/negative fixtures; output preservation on parse/type/target/build failures; Unicode source spans; debug/release outputs; installed execution; existing interpreter/native/Rust/Python parity. Read-only notebook/advisory regressions.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-cli`, `cgx2c`, `cgx2-emit-rust`, `cgx2-emit-python`, `cgx2-api-index`, `cgx2-package`, `cgx2-sema`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

An installed-command acceptance script plus owned evidence report. List remaining `test`, `verify`, `notebook`, `bind` integration requirements explicitly.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
