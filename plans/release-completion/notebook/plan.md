# CGx2 retained notebook runtime contracts

ID: S1-08. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Support runtime-checked requires/ensures on retained notebook functions, including permitted local accumulator functions, with correct dependencies, cost limits and failure behavior.

## Exclusive write ownership

- `crates/cgx2-notebook-format/**`
- `crates/cgx2-notebook-session/**`
- `crates/cgx2-notebook-kernel/**`
- `crates/cgx2-notebook-cli/**`
- `crates/cgx2-notebook-ui/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Use the existing checker/interpreter contract semantics; track identifiers and calls used only in pre/postconditions for invalidation, source diagnostics and provenance.
2. Account for contract evaluation in static cost and retained replay. Preserve 100000 steps, 32 functions/depth, 1 MiB limits and worker deadlines unless a demonstrated bug requires an explicitly documented repair.
3. Check result binding, false/overflowing contracts and forbidden effects before publishing values. Preserve transactional notebook bindings on errors and direct/worker parity.
4. Exercise persistence, selected rerun, restart/run-all, sweep and HTTP/UI behavior. Imported functions retain their checked profile; no unsupported effect admission.
5. Do not integrate the concurrent worker crate yet. S3 explicitly supplies persistent execution-once notebook integration after contracts/core/worker bases are reviewed.

## Acceptance and checks

Both pre/postcondition failures; MIN/MAX; edited/deleted contract dependencies; contract-hidden effects; worker timeout/errors; no partial bindings; saved provenance/staleness; all owned suites and 23+ HTTP regressions.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-notebook-format`, `cgx2-notebook-session`, `cgx2-notebook-kernel`, `cgx2-notebook-cli`, `cgx2-notebook-ui`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

Runnable .cgnb contract examples and owned evidence report, with exact remaining effectful-notebook blocker.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
