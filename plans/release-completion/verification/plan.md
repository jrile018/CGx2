# CGx2 useful scalar proof profile and live solver runner

ID: S1-09. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Expand bounded scalar source verification to immutable locals and checked multiplication/division, with independently validated source/core/SMT behavior and a real-solver acceptance runner.

## Exclusive write ownership

- `crates/cgx2-verify-core/**`
- `crates/cgx2-verify-cli/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Lower immutable local bindings with lexical scopes and checked multiplication/division semantics. Add literal-bound local accumulator expansion only with an explicit expansion bound and parity coverage; keep this reported separately if blocked by current contract admission.
2. Preserve independent finite audit, solver protocol validation, witness validation and explicit proved/disproved/unknown meanings. No solver text or annotation alone grants proof.
3. Keep current limits documented: max-cases 100000, timeout-ms 2000 per solver process, input 65536 bytes. Bound expansion before solver startup.
4. Provide trusted/pinned Z3/CVC5 acceptance runner that captures executable hash/version, SMT hashes, responses, probes and witnesses. Missing solver is blocked/unrun, never success.
5. Array bounds, annotated loop invariants and new variant/generic types are S3 dependencies on S2 syntax/core; reject them explicitly now.

## Acceptance and checks

Valid/false/overflow/divide-by-zero/MIN divided by -1; inactive branches; shadowing; expansion limit; forged/malformed/timeouts; independent oracle corruption; real pinned solver when available.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-verify-core`, `cgx2-verify-cli`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

Exact proof subset, acceptance runner and S3 invariant/array integration requirements; distinguish runtime tests, finite audit and trusted SMT results.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
