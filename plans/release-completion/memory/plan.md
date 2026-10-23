# CGx2 fallible runtime and lifetime completion

ID: S1-02. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Close runtime bookkeeping gaps with fallible session/guard entry and compiler-consumable scoped lifetime cleanup, keeping existing entry points compatible.

## Exclusive write ownership

- `crates/cgx2-runtime/**`
- `crates/cgx2-memory-conformance/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Add versioned/additive fallible entry points for session and no_gc guard creation. Audit each new reserve site and expose deterministic fault injection without claiming process-wide OOM recovery.
2. Provide scoped-root and cleanup infrastructure that releases roots once, preserves the original failure, and works across nested scope exit, cancellation and unwind. Existing stable handles, region limits and ownership conversions retain their behavior.
3. Extend the state-machine corpus to every new failure transition. Manual regions never collect implicitly and nested no_gc prohibits all CGx2 collection even on allocation failure.
4. Supply a leak-enabled acceptance runner that records actual sanitizer configuration and supports an external capable host. A ptrace restriction or missing tool is blocked; detect_leaks=0 is a separate non-leak result.

## Acceptance and checks

Owned suites and fault corpus; root census before/after errors, failed guard entry, repeated cleanup, cross-region isolation and failure inside nested guards; targeted ASan when available. No source-compiler edits.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-runtime`, `cgx2-memory-conformance`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

Document exact APIs and cleanup invariants for S2 ordinary managed/owned source lowering; list allocator/foreign allocations outside the guarantee.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
