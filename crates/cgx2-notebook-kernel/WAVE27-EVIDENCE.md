# Wave 27 notebook declared-import closure

Base: `e3a01dffee423ab93d9cc8cd6d69c0f910c667f4`. Worktree:
`/home/john-riley/.codex/worktrees/6df7/lang`. Only notebook crate source,
tests, examples, documentation and evidence are included in this deliverable.
No root Cargo files, frozen package/syntax/check/run APIs, coordinator ledger or
other-lane files changed.

## Implementation

- `src/project.rs` validates the checked package's declared graph against its
  dependency-first topology. Iterative discovery selects the complete closure,
  deduplicates diamonds/multiple roots and rejects missing, cyclic, duplicate or
  inconsistent metadata. Graph size/path depth are bounded by
  `cgx2_package::MAX_SOURCES = 128`. Each selected module is lowered once into a
  `OnceLock` cache; unrelated unsupported modules are not lowered.
- Executable definitions and provenance use the same closure function. Source
  assembly checks the deduplicated combined 32-function/1,048,576-byte budgets.
  Replacement/append checks run before source growth. Local retained definitions
  share these budgets, and diagnostics include combined function/source counts.
  No per-module budget increase was introduced.
- Direct notebook imports remain the sole namespace for explicit calls,
  constructors and type annotations. Public record values can flow from an API
  through an internally included owner, retain their full nominal identity, and
  support scalar-field projection/replay without exposing that owner's names.
  Cross-module source calls still require the calling package module's own
  explicit imports and public declarations, enforced by the frozen package gate.
- Every selected function (including private helpers and inactive branches) and
  every declared dependency (including unused imports) passes the pure profile.
  The frozen package checker conservatively infers `Unknown` for qualified calls
  and rejects explicit `effects()` on wrappers. Such wrappers omit the annotation;
  notebook lowering proves their supported bodies pure and the complete script
  passes the shared checker again. Nonempty declared effects/capabilities,
  contracts, foreign bindings and unsupported/top-level bodies remain rejected.
- `src/lib.rs` propagates package dependencies through file-order symbol producers
  into local calls and retained-value/projection users. Independent observations
  remain independent. Live checked snapshots still require reopening after any
  detected package mutation; no metadata is silently reloaded.
- The format crate adds optional `PackageProvenance.module_paths_sha256`, serialized
  as `module_path_sha256=name:digest,...`. Keys must exactly match source-hash
  module keys. A digest hashes `CGX2-PACKAGE-MODULE-PATH-v1:` plus lowercase hex
  canonical-path bytes. Source hashes remain actual source hashes. This catches
  same-byte canonical-target changes across save/reopen. Older captures remain
  readable and package captures without this metadata become stale. Root digest
  and manifest provenance retain their existing meaning.

## Regression coverage

`tests/import_closure.rs` adds 12 integration tests. `project.rs` adds three unit
tests, and CLI/HTTP each add one lifecycle regression. The shared readable fixture
is `../cgx2-notebook-cli/examples/import-closure/import-closure.cgnb`, with its
six-module local package: a three-level chain, diamond, private helper, independent
same-shaped record and unrelated module.

Coverage includes interpreter/worker execution; two roots sharing a dependency;
nominal same-named records; direct versus internal visibility; annotation-only,
unused and inactive dependencies; Unicode record/function/field identifiers;
named argument evaluation order and checked arithmetic; private/missing/unlisted/
cyclic/reserved-name failures; source, schema, visibility, import-edge, deletion,
manifest, root and same-byte canonical-target edits; live/saved selective
invalidation; cancellation/zero timeout and recovery; malformed/wrong-identity/
wrong-field record frames and transactional binding publication; save/reopen,
selected/affected rerun, restart, CLI clean sweep, export and HTTP lifecycle.

Boundary tests admit exactly 128 declared levels/modules and reject 129; accept a
deduplicated 32-function graph and reject its 33rd function; reject two modules
whose combined lowered source exceeds 1 MiB; and test edited/appended source at
exactly 1 MiB and one byte beyond before source mutation. Existing step, depth,
frame, cell, retained-state, effect and argument-order suites also pass.

## Exact final checks

Toolchain: `rustc 1.98.1 (48a229cea 2026-09-01)`,
`cargo 1.98.1 (797e8a9bc 2026-08-05)`. Commands ran from the worktree above.
Private build/temp directories stayed under that worktree. No bridge linking
workaround or `LIBRARY_PATH` override was needed.

```sh
CARGO_TARGET_DIR=$PWD/.wave27/target TMPDIR=$PWD/.wave27/tmp cargo +1.98.1 test --offline -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui --all-features
CARGO_TARGET_DIR=$PWD/.wave27/target TMPDIR=$PWD/.wave27/tmp cargo +1.98.1 clippy --offline -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui --all-targets --all-features -- -D warnings
CARGO_TARGET_DIR=$PWD/.wave27/target TMPDIR=$PWD/.wave27/tmp cargo +1.98.1 fmt -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui -- --check
git diff --check
```

All four commands exited 0. **184 tests passed**, including **22 loopback HTTP
integration tests**, with zero failed/ignored tests. Raw output is in
`evidence/wave27-tests.txt` and `evidence/wave27-clippy.txt`; format verification
is recorded in `evidence/wave27-format.txt`. The sandbox initially denied socket
binds (`EPERM`); the successful final test command ran with approved localhost
socket access. The denied development run is not counted as passing.

## Limits

Pure scalar/immutable scalar-field records only; no effect replay, reexports,
cycles, mutable/generic/nested records or general library support. Package wrappers
with an explicit empty effect annotation on cross-module calls remain rejected
by the unchanged package checker. Package sources and `cgx2.toml` remain external;
exported notebooks need the matching `--project ROOT` and are not self-contained
package bundles. Older binaries may reject the new optional provenance field.
Snapshot checking and in-process parsing are bounded but are not made preemptible
by worker timeouts. No LeakSanitizer, live solver, delegated cgroup containment,
real solver/foreign runtime gate, paired human/model trial, whole-workspace suite
or memory-lane validation was run or claimed by this notebook-only deliverable.
