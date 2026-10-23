# CGx2 persistent worker transport

ID: S1-06. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Implement the bounded persistent local-child transport needed by effectful notebook and ecosystem adapters, using the frozen protocol in contracts.md.

## Exclusive write ownership

- `crates/cgx2-interop-worker/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Build explicit session/request identifiers, authenticated private child channels, bounded length frames, runtime identity handshake, deadlines, cancellation, crash handling, and complete child/process-group cleanup.
2. Implement at-most-once dispatch per live session: reject duplicate IDs and never automatically retry an uncertain request after timeout/crash. Preserve an explicit outcome_unknown status and session poisoning where appropriate.
3. Keep persistent child state and transport separate from notebook semantics. Supply a deterministic fixture child with a counter/write effect proving no transport replay. This is not completion of effectful notebooks.
4. Expose requested capability policy and actual enforcement status separately. Restricted mode must return unavailable when required OS confinement is unsupported; allowlists alone are not confinement. No host policy changes or package installation.
5. Do not depend on the concurrently changing native SDK. Use the contract's wire values internally behind an additive API; S2 owns the adapter to the reviewed shared ForeignValue representation.

## Acceptance and checks

Malformed/oversized/truncated/duplicate/unsolicited frames; identity/auth mismatch; crash during dispatch; timeout and cancellation; descendants cleaned up; restricted-host pass/unsupported branches; real filesystem/network denial only on capable hosts.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-interop-worker`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

Protocol test vectors, lifecycle state machine and integration API for notebook/Node/PHP consumers. std-only dependencies preferred; coordinator reconciles lock changes.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
