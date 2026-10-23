# CGx2 executable release evidence pipeline

ID: S1-11. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Deliver one executable release-evidence command that runs or imports real isolated submissions and runtime/resource observations, validates provenance, and reports passed/failed/blocked/unrun per release gate.

## Exclusive write ownership

- `bench/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Compose existing isolation selftest, scorer, public task export, genuine trace ingestion, native/LibTorch measurements and confidence reporting instead of creating duplicate engines.
2. Require immutable result bundles with compiler/source/tool/environment hashes, commands, exit codes, raw artifacts and review status. Reject tampering, incomplete pairs and missing evidence. Historical results stay immutable.
3. Implement real external-worker handoff recipes for delegated cgroups, trusted solvers and leak-enabled checks; the latter two invoke owner-supplied runners after integration. Local absence remains blocked.
4. Preserve full failure-budget accounting, task-cluster paired uncertainty, baseline separation, held-out test separation and independently reviewed task status. Two model families and genuine human/blinded assessments remain actual data requirements.
5. Run functional harness checks locally. Do not run release performance benchmarks alongside 12 compiling chats. Supply a queue/quiet-host prerequisite and operator command, not a scheduled automation.
6. No invented model traces, human decisions or safety approvals. Do not contact people, spend on external model APIs, install packages or alter host isolation settings.

## Acceptance and checks

All 122+ Python unit tests; result schema validation; tampered/missing provenance; unavailable host; incomplete studies; real isolation positive/adversarial checks only when supported; deterministic reporting of blocked gates.

Run the Python/tool tests relevant to the owned deliverable and its actual executable acceptance path. Do not write fixtures into other lanes.

## Commit and stop

One documented command, example clearly labeled fixture bundle, external execution checklist and complete finite empirical gate inventory.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
