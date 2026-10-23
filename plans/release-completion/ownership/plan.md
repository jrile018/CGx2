# CGx2 ownership analysis for application functions

ID: S1-03. Status: ready for dispatch from committed common base. Read [master plan](../README.md), [worker rules](../worker-rules.md) and [frozen contracts](../contracts.md).

## Goal

Add a bounded source-spanned ownership analysis API for ordinary function calls, aggregate projections and parameter/result loans, preserving current `Op` and `check_ops` consumers.

## Exclusive write ownership

- `crates/cgx2-ownership/**`

All other paths are read-only, including coordinator plan files and root Cargo manifests/lock. Evidence and examples go inside an owned path.

## Required deliverable

1. Represent function summaries, owned transfer, shared/exclusive loans, parameter/result relationships, aggregate projections and region escapes in an additive module.
2. Implement conservative CFG joins and terminating loop fixed points with explicit analysis limits. Existing basic Loop support already exists; do not repeat it as the deliverable.
3. Track foreign owner retention and reject unknown lifetime assumptions. Model partial moves and projection overlap consistently; no implicit cross-region aliases.
4. Produce an adapter recipe and fixtures for S2 compiler lowering. This library delivery cannot claim ordinary `.cg` borrowing is implemented.

## Acceptance and checks

Valid loans/calls; use after move; returned local borrow; partial-move misuse; conflicting projections; loop-carried invalid loans; region escape; unknown foreign retention; correct UTF-8 spans; budget exhaustion terminates conservatively.

Run offline all-feature tests and all-target/all-feature Clippy with warnings denied for `cgx2-ownership`. Include any new owned crate. Use a private target and TMPDIR per worker-rules; CARGO_BUILD_JOBS=2. Run formatting only for owned packages and git diff --check.

## Commit and stop

Versioned analysis types, examples of compiler inputs/diagnostics, and an explicit supported-semantics boundary.

Record actual commands, exit codes, supported versus blocked external checks, source versus library scope, files and commit identifier. Update MegaMemory, commit your bounded deliverable on your worktree branch, and stop. Do not start dependent Stage 2/3 work, merge main, or create more chats.
