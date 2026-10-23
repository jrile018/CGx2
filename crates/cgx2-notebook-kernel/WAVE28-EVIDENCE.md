# Wave 28: notebook-local accumulators and literal ranges

Base: `d82aa8583a922d83f92a95cbe8462ef890e44c1e`.
Worktree: `/home/john-riley/.codex/worktrees/7402/lang`.
Only owned notebook crate source, tests, examples, READMEs and evidence changed.
Root Cargo files, shared design/roadmap/ledger documents and other lanes are
unchanged. No branch merge or next wave is included.

## Implementation and profile

- `src/lib.rs` verifies every notebook function's supported statements and
  lexical bindings. Mutable locals must have shared-checker initializer type
  `i64`, including unused locals in zero-trip loops/inactive branches. `=` and
  `+=` may target only a currently visible mutable binding declared in that
  function. Branch/loop environments clone at entry and discard declarations
  on exit; immutable shadowing and loop-index shadowing cannot grant mutation.
  Parameters, captures, imported/module names and record/field writes reject.
- `range(start, end)` requires exactly two positional signed i64 syntax
  literals. The ascending half-open loop is empty when start >= end. Nested
  loops and mandatory-else conditionals use the frozen interpreter. Returns
  inside loops, dynamic bounds/iterators/steps and unsupported statements reject.
- Only explicit/inferred local `Mutate` is admitted after complete independent
  structural checking and shared type/effect checking. An explicit `effects()`
  on mutation remains an error. All extra effects/capabilities, foreign, memory,
  no_gc and contract operations remain rejected. Calls outside loops may reach
  other fully verified local computational functions. The frozen checker keeps
  its transitive pure-helper-only rule inside loops, including empty loops;
  mutating helpers cannot pass through wrappers.
- Call discovery, external reads and annotations now recurse loops and
  assignment RHS expressions. Local targets/indices are not cell dependencies.
  Inactive/empty bodies still resolve and invalidate all read functions/types.
- `src/project.rs::lower_statement` lowers local loop bodies, annotations and
  assignment RHS calls and checks index/module-root ambiguity. Imported module
  `validate_signature`/`validate_block` admission is unchanged from Wave 27:
  no newly admitted mutable or loop bodies/nonempty effect declarations.
  Public pure scalar helpers may execute inside notebook-local loops.
- `src/cost.rs` replaces the flat replay heuristic with a memoized call model.
  Each statement/expression costs one step; a range costs four setup steps plus
  `trip_count * (1 + body_steps)`. Both conditional branches count. Trip
  differences use i128 and multiplication/addition saturate `usize`. Empty
  bodies still resolve unknown/recursive calls and static depth. Cached costs
  retain depth, preventing dependency-order bypass and repeated DAG expansion.
  Each actual retained prefix is parsed/costed for every generated invocation.
  Each earlier pending binding costs conservatively at most 42 steps per later
  invocation: let + constructor + eight MIN i64 fields (five expressions each).
  A generated result read costs two steps. The admission limit stays 100,000
  before any worker starts or definitions/pending values publish.
- Execution still uses `check_source`, `run_source` and the tagged worker;
  no source unrolling, alternate evaluator or timeout substitution. Existing
  transaction, worker type/schema validation and source-map handling remain.

## Coverage

Four cost-model unit tests and sixteen `tests/local_loops.rs` integration tests
cover direct/worker execution, explicit/inferred mutation, both assignment
forms, negative/empty/descending/one/multiple/MIN/MAX literal ranges, nested
loops/ifs, lexical shadowing/scopes, parameter/index/capture/record writes,
wrong types, dishonest pure signatures, hidden mutating wrappers, forbidden
effects/operations and imported Wave 27 rejection. Checked add/multiply/divide
failures, partial-cell rollback, interruption after worker start, zero deadlines,
missing workers and malformed/wrong-type frames recover through a clean run.
Earlier independent outputs/provenance are preserved.

The exact 100,000-step invocation runs through both direct and worker paths;
99,999 accepts and 100,001 rejects in the model. Generated result retrieval,
MIN/record replay, combined calls, nested-loop multiplication and the complete
MIN..MAX range reject before worker startup. The test uses a nonexistent worker
to distinguish static rejection from attempted execution. Existing 32-function,
call-depth, byte-budget, worker-frame and package closure tests also pass.

Dependency tests cover assignment RHS calls, inactive branches, empty loops and
annotations, helper deletion/edits and schema edits. The readable example is
`../cgx2-notebook-cli/examples/local-loops/local-loops.cgnb`, with one pure local
package helper. Kernel tests exercise selected/affected rerun, save/reopen and
Restart and Run All. A CLI regression checks edit, save/reopen, export and three
clean sweep trials against separate clean executions. An HTTP regression checks
loopback worker execution, parameter edit/affected rerun, independent provenance,
save/export/reopen and restart. Existing HTTP interrupt/security/lifecycle
regressions remain passing.

## Exact final verification

Toolchain: `rustc 1.98.1 (48a229cea 2026-09-01)`;
`cargo 1.98.1 (797e8a9bc 2026-08-05)`.
All build and temp files stayed in the private `.wave28/target` and
`.wave28/tmp` beneath this worktree; no shared checkout target was used.

From the worktree root:

```sh
CARGO_TARGET_DIR=$PWD/.wave28/target TMPDIR=$PWD/.wave28/tmp cargo +1.98.1 test --offline -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui --all-features
CARGO_TARGET_DIR=$PWD/.wave28/target TMPDIR=$PWD/.wave28/tmp cargo +1.98.1 clippy --offline -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui --all-targets --all-features -- -D warnings
CARGO_TARGET_DIR=$PWD/.wave28/target TMPDIR=$PWD/.wave28/tmp cargo +1.98.1 fmt -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui --check
git diff --check
```

Results: **207 passed, 0 failed, 0 ignored**, including **23 HTTP tests**;
Clippy and formatting exit 0. Final logs are `evidence/wave28-tests.txt`,
`evidence/wave28-clippy.txt`, `evidence/wave28-format.txt`. An initial HTTP bind
was denied by the sandbox with EPERM; final suites ran with approved localhost
socket access. That denied development run is not counted as passing.

## Limits

Only notebook-defined functions gain this local i64 computation profile.
Externally effectful workflows, captured mutable state, generic/nested/mutable
records and general iterators remain unsupported. Frozen pure-helper policy
also rejects record constructors in range bodies, including constructors hidden
inside helpers; existing record parameters and projections remain usable.
Pending-result bounds are conservative and may reject cells whose particular
runtime values would cost fewer steps. Existing limits remain 128 code cells,
64 KiB source, 32 functions/static call levels, eight parameters, 1 MiB
cumulative evaluation/retained state and bounded worker frames. Default worker
deadline stays two seconds; frozen runtime limits stay 1,000,000 steps/64 call
levels. In-process parsing/checking is bounded but not preemptible by worker
interrupts. Reopen restores cached observations, so prerequisites must rerun
before saved definitions can be called. Export embeds notebook assets; matching
package source and `cgx2.toml` stay external and require `--project ROOT`.

No whole-workspace/package-lane validation, general memory/foreign release
assurance, live solver, benchmark, GPU, or paired human/model trial is claimed.
