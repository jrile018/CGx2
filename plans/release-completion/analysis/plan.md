# CGx2 package effect and security analysis

ID: S1-10. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Add package-aware effect/capability analysis and bounded intermodule taint propagation using the reviewed Wave 28 checked package summaries.

## Exclusive write ownership

- `crates/cgx2-effect-index/**`
- `crates/cgx2-security/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Consume load_checked_package and typed body-derived summaries with checked source provenance; do not fabricate or duplicate trusted imported-effects evidence.
2. Resolve explicit public call identities across package edges, propagate effects/capabilities and bounded taint, and report unknown for unresolved/computed/over-budget cases.
3. Preserve single-file advisory behavior and deterministic JSON/SARIF with package-relative UTF-8 spans and related declarations.
4. Treat branches, loops, shadowing and transitive forwarding conservatively. Analysis output is advisory and does not authorize runtime capabilities or prove vulnerability absence.

## Acceptance and checks

Pure imported wrappers; hidden transitive effects/caps; private/unimported/duplicate identities; clean literals and taint forwarding; inactive branches and loop-carried uncertainty; budget exhaustion; source mutation/provenance; stable SARIF.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-effect-index`, `cgx2-security`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

Package examples and per-rule supported/unknown behavior, plus S3 new-type/foreign-call adaptation needs.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
