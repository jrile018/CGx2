# S1-08 retained notebook runtime contracts

Implemented and locally verified on 2026-10-23 in the attached worktree
`/home/john-riley/.codex/worktrees/3fca/lang`, branch
`riley/release-notebook-contracts`, based on reviewed commit
`0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
The delivery commit is the commit containing this report; resolve it with
`git log -1 --format=%H -- crates/cgx2-notebook-kernel/S1-08-EVIDENCE.md`.
Its exact identifier is also reported with delivery to the user.

## Implemented scope

- `src/lib.rs`: retained notebook functions admit checked `requires`/`ensures`.
  Contract facts include parameter arguments and contract-only calls, while only
  postconditions see `result`. Captured cell bindings/body locals remain forbidden.
  Contract calls participate in opaque-effect detection, including unrun unknown
  calls. An unchanged, failed admission can use the existing saved-safe-rejection
  rule without poisoning independent outputs. No schema change was needed.
- `src/cost.rs`: function cost includes all pre/postconditions and their transitive
  calls on each generated invocation, including after constant-false conditions.
  Recursion and cached static call depth include contract-only edges. Existing
  retained/pending replay charging and saturation remain in effect. Tests exercise
  99,999, 100,000 and 100,001 steps, plus replay/retrieval crossing the boundary.
- `src/project.rs`: local notebook pre/postcondition calls to checked public imported
  predicates are lowered through the existing package adapter and inverse source
  map. Dependency facts preserve manifest and transitive module provenance.
  Imported functions retain their frozen profile and still reject contracts,
  mutation/loops, foreign bindings, capabilities and observable effects.
- `tests/contracts.rs`, `src/cost.rs` tests, CLI `tests/command.rs` and UI
  `tests/http.rs`: true/false pre/postconditions, explicit returns, Boolean result,
  MIN/MAX, contract arithmetic overflow/division failure, result scoping, malformed
  conditions, hidden mutation/unknown effects, dependencies used only in contracts,
  edit/delete staleness, persistence, recovery, no pending bindings, direct/worker
  parity, static limits, CLI sweeps/portable export and HTTP actions.
- Runnable source examples: `../cgx2-notebook-cli/examples/runtime-contracts.cgnb`
  and `../cgx2-notebook-cli/examples/contract-imports/contracts.cgnb` with its checked
  two-module `.cg` package. All examples are exercised by integration tests.
  The old local-loop test expecting every contract to reject was removed.

This is **source-level `.cgnb` execution support** through the parser, checker,
interpreter, CLI and UI, not a library-only contract evaluator. Functions are
ordinary CGx2 syntax. The shared checker limits contracted signatures to annotated
`i64`/`Bool` parameters/results; String/record contracts remain rejected. Pure local
or checked imported predicates are allowed; a function with verified local mutable
`i64` accumulators can have contracts, but a mutating helper cannot be a predicate.

The interpreter remains the authority: requires runs before the body, ensures
checks the returned value (including explicit returns), false conditions report
`CGR015`/`CGR016`, and arithmetic errors keep their existing codes. Static diagnostics
point to the condition in the source cell. Runtime diagnostics point to the calling
statement's source span and match direct/worker execution; the worker error protocol
and all frozen public APIs/schemas are unchanged. Runtime checks are not proofs.

Limits remain 128 code cells, 64 KiB code, 32 functions, eight parameters,
32 static call levels, 100,000 static steps including contracts/replay,
1 MiB evaluation/retained state and a default two-second worker deadline.
Failure/timeout/interrupt publishes no pending bindings or partial definitions;
prior displayed values remain failed/stale observations. Existing worker
timeout/interrupt/malformed-frame and byte-limit regressions ran in the full suite.

## Verification

Environment: Rust/Cargo 1.98.1. All builds used:

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-notebook"
export TMPDIR="$PWD/target/tmp-notebook"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
```

Commands below ran from the attached worktree, with exit code **0** each:

```sh
cargo test --offline --all-features -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui
cargo clippy --offline --all-targets --all-features -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui -- -D warnings
cargo fmt --check -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui
git diff --check
```

**224 tests passed, zero failed/ignored**, including all **24 HTTP regressions**
(the 23 existing regressions and a new contract lifecycle test in direct and worker
modes). CLI: 30 tests. Kernel: 131 tests including 12 contract integration cases
and two new cost cases. Format: 22 tests. Session: 16 tests. UI: 25 tests.
Logs: [tests](evidence/s1-08-tests.txt), [Clippy](evidence/s1-08-clippy.txt),
[format](evidence/s1-08-format.txt), [diff](evidence/s1-08-diff-check.txt).

The first sandboxed HTTP attempt exited 101 because `TcpListener::bind` returned
`PermissionDenied`. Final HTTP and full-suite commands ran successfully with local
loopback permission; this was not treated as a test pass or a host-policy change.
Development tests also corrected fixtures for the existing no-mixed-function/script
grammar, explicit rerun selection, open-time imported-profile rejection and HTTP
error responses requiring a separate state read. No production workaround or
acceptance exclusion was introduced for those test-fixture errors.

No dependencies, Cargo manifests, root lockfile, shared compiler files, plans or
coordinator ledgers changed. `cgx2-notebook-format` and `cgx2-notebook-session` needed
no code changes; their existing state/provenance behavior was exercised unchanged.
Serena's generated untracked project/cache directory was removed before delivery.

## Limits, blockers and executable handoffs

No external acceptance is needed for this bounded pure/local contract deliverable.
This evidence covers real local executions and deterministic test fixtures; it
does not claim whole-language coverage or release readiness.

**Persistent effectful notebook acceptance remains blocked on S2/S3 integration**:
reviewed compiler effect/capability/type adoption and the independently developed
persistent local-child worker must be connected before external effects/handles can
be admitted. Earlier effectful cells must never replay to reconstruct state; each
selected run must dispatch once, publish only validated results, and mark a
post-dispatch timeout/crash as unknown/stale without retry. Foreign handles must
remain session-bound and release on cancellation/restart. This deliverable does
not import the concurrent worker crate, enable file/network/Python/tensor/managed
effects or start that dependent work. Imported contract-bearing APIs remain an
explicit boundary, not a newly admitted effect route.

Executable correctness handoff, after building the owned CLI/UI:

```sh
cargo build --offline -p cgx2-notebook-cli -p cgx2-notebook-ui
"$CARGO_TARGET_DIR/debug/cgx2-notebook" run crates/cgx2-notebook-cli/examples/runtime-contracts.cgnb --out "$TMPDIR/contracts.cgnb"
"$CARGO_TARGET_DIR/debug/cgx2-notebook" inspect "$TMPDIR/contracts.cgnb"
"$CARGO_TARGET_DIR/debug/cgx2-notebook" sweep crates/cgx2-notebook-cli/examples/runtime-contracts.cgnb parameter --value -1 --value 0 --value 5 --out-dir "$TMPDIR/contract-sweep"
"$CARGO_TARGET_DIR/debug/cgx2-notebook" run crates/cgx2-notebook-cli/examples/contract-imports/contracts.cgnb --project crates/cgx2-notebook-cli/examples/contract-imports --out "$TMPDIR/imported-contracts.cgnb"
"$CARGO_TARGET_DIR/debug/cgx2-notebook-ui" open "$TMPDIR/contracts.cgnb" --port 0
```

Use fresh output paths on subsequent runs (or the CLI's explicit overwrite option).
Expected values are 8 for the default example and 6/11 for sweep values 0/5;
the -1 trial records `CGR015`, saves failed provenance and publishes no result.
For the existing effect-replay rejection boundary, run:

```sh
cargo test --offline -p cgx2-notebook-kernel selected_record_function_execution_never_replays_an_effect_cell
cargo test --offline -p cgx2-notebook-kernel --test contracts imported_functions_keep_their_frozen_contract_and_effect_rejection_profile
```

These negative checks are not positive evidence of persistent external-effect
execution. After the S2/S3 owner adds that integration, the full owned command above
is the regression handoff alongside new real-worker execution-count/unknown-outcome
acceptance. Benchmarks were unrun: no quiet-host timing claim is made. Leak-enabled
memory/FFI assurance, live trusted solver, scored isolation and paired human/model
release studies remain outside this bounded lane and unrun; no fixtures or runtime
contract tests substitute for their separate release gates.
