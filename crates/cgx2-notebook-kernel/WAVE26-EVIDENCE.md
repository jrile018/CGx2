# Wave 26 notebook public-record handoff

Base: `a574cea65d801c284e86c057bbab3fdd7ebb4549`.
Worktree: `/home/john-riley/.codex/worktrees/5081/lang`.
Toolchain: `rustc 1.98.1 (48a229cea 2026-09-01)`.
Implementation and this report are committed together; the resulting commit identifier
is reported in the chat handoff (a commit cannot embed its own identifier).
Only the five `crates/cgx2-notebook-*` crates were changed. No frozen package,
syntax, CLI/compiler/runtime API or shared/root Cargo file was edited.

## Delivered

- Checked public scalar-record metadata is consumed via frozen
  `CheckedPackage::modules()`, `CheckedModule::syntax()`, source hashes/canonical paths,
  API indexes and `verify_unchanged()`. No missing accessor blocked this deliverable.
- Explicit direct imports admit public records with 1–8 immutable i64/Bool fields,
  named constructors, field projection, record parameters/returns and local pure
  functions using imported annotations. Module-qualified nominal identities survive
  internal lowering, retained values, checked worker transport and display.
- Every statement's returned type is checked against the frozen CLI typed summary;
  matching schemas alone cannot admit a same-shaped record from another module/local
  declaration. Constructor argument evaluation order and checked arithmetic are retained.
- Annotated declarations, constructors, projections, local/imported function bodies
  and both conditional branches participate in dependencies. Detected module changes
  discard matching live definitions and stale transitive readers; manifest failures
  affect all package users. Independent captures keep status/output/provenance.
- Canonical root digest supplements exact manifest and module source digests.
  Old schemas 1/2/local-record captures remain readable; old package captures without
  a root digest become stale. Duplicate/malformed root fields reject.
- Package snapshots are checked before execution/replay and after workers, with another
  check before definition/binding publication. Changed inputs require reopening;
  the kernel never silently reloads stale metadata. Original-source diagnostic offsets
  are restored after qualified-name expansion.
- CLI run/selective rerun/restart/inspect/delete/sweep, save/reopen, export and loopback
  UI paths are exercised. Sweep trials use clean kernels and `--project ROOT`.
  The readable example and small package are in
  `crates/cgx2-notebook-cli/examples/public-records/`.

## Exact final verification

Executed from the worktree above, using private directories under it:

```sh
mkdir -p .wave26-target .wave26-tmp
export CARGO_TARGET_DIR="$PWD/.wave26-target"
export TMPDIR="$PWD/.wave26-tmp"
cargo +1.98.1 test --offline --all-features -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui > .wave26-tmp/final-tests.log 2>&1
cargo +1.98.1 clippy --offline --all-features -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui --all-targets --no-deps -- -D warnings > .wave26-tmp/final-clippy.log 2>&1
cargo +1.98.1 fmt --check -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli -p cgx2-notebook-ui
git diff --check
rustc +1.98.1 --version
```

All final commands exited 0. **167 tests passed, 0 failed**, including 21 HTTP tests.
Full test and Clippy output is retained in [evidence/wave26-tests.txt](evidence/wave26-tests.txt)
and [evidence/wave26-clippy.txt](evidence/wave26-clippy.txt).

| Crate | Passed |
| --- | ---: |
| notebook-format | 22 |
| notebook-session | 16 |
| notebook-kernel | 81 |
| notebook-cli | 26 |
| notebook-ui | 22 (including 21 loopback HTTP) |

The sandbox denied loopback binding on the initial combined run. The final combined
suite was run with approved loopback permission and passed. Early development runs
also exposed the old public-record rejection assertions, same-cell local replay,
a fixture without a display expression, and diagnostic offset shifts; those were
corrected and are covered by the final tests. One intermediate command used an
invalid Cargo `--exclude` option and was replaced by the final command above.

The new kernel suite covers identical module/local record names, reordered fields,
unused imported annotations, inactive branches, explicit qualified constructors/types,
wrong nominal arguments, private/unknown/missing imports, ambiguous module roots,
Unicode names, generated-name collisions, module edit/delete/visibility/field/manifest
changes, canonical target replacement with identical bytes, package-root changes,
selective invalidation, wrong worker identities/malformed fields, failure recovery,
no hidden effect replay, interrupt/zero deadline, save/reopen/restart and export.
CLI and HTTP tests cover the supplied package fixture across their lifecycle routes.
Existing suites retain static step/depth/replay bounds and effect rejection coverage.

## Resource and support limits

The previous bounds remain: 128 code cells, 64 KiB cumulative notebook code, 32 pure
functions, 8 function parameters, 32 static call levels, 100,000 static estimated
steps, 1 MiB cumulative evaluation/replay/retained-state budget, and 65,536 bytes per
captured text output. Records have 1–8 i64/Bool fields. Record identity/field identifier
strings are at most 65,535 UTF-8 bytes and worker frames at most 1 MiB. Worker default
deadline remains 2 seconds; parent-side parsing/checking is not wall-clock preempted.
The frozen evaluator retains its independent step/depth limits. Source/hash failure
classification uses the frozen package's bounded source size and canonical paths.

No transitive-import expansion, reexports, private-record access, nested/generic/
managed/foreign record support, contracts or effects were added. Imported functions
retain their existing bounded pure call profile; named function arguments are rejected.
No prior effectful cell is replayed to reconstruct state. Notebook assets can be
exported, but package source/manifest are external: exported imported-record notebooks
still require `--project ROOT`. No self-contained package, performance, foreign-memory,
release-wide safety, live solver, LeakSanitizer, cgroup or real tensor claim follows.
No bridge linking override, network dependency download or live solver was needed.

Build artifacts/log scratch directories are moved under ignored `target/wave26-*`
after verification to leave only owned source/evidence changes for review.
