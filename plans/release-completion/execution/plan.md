# CGx2 cancellable CPU execution and profiling

ID: S1-07. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Finish a uniform cancellable owned-input map/zip execution and profiling API for the S2 source pipeline. Use existing primitives; do not add arbitrary kernels.

## Exclusive write ownership

- `crates/cgx2-execution-policy/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Normalize the reusable execution entry points around existing map/zip primitives with explicit cancellation, owned inputs and atomic publication of results.
2. Expose structured operation-specific measured budget/performance reports and validated profile selection. Validate operation, machine, size, age and evidence quality; Auto remains sequential without trustworthy measurements.
3. Preserve deterministic lowest-index errors and sequential checked arithmetic semantics. Do not reassociate reductions, share managed runtime sessions across threads, or advertise GPU support.
4. Document the existing execute_zip_map_cancellable source consumer contract, including copy/allocation/thread/join costs, so S2 can expose a bounded owned-input source primitive. This is the concrete consumer requirement for this lane.
5. Run only functional/small diagnostic measurements concurrently. Release benchmark measurements are owned by bench and must run on a quiet host.

## Acceptance and checks

Sequential/parallel values and error index; cancellation before/during execution; no partial output or surviving workers; stale/mismatched/noisy profiles; unsupported explicit backends; budgets and integer boundaries.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-execution-policy`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

Stable additive API and report schema with source integration fixture/example; no speedup claim from noisy local timing.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
