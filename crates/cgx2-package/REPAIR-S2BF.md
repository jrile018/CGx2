# S2b package exported generic binder repair

Bounded repair from the exact reviewed base
`b595724e1b086c604220cc5b165c141e65a0531f`, on
`riley/s2b-package-binders` in the attached worktree. The foundation remains
repair-required pending the other lanes and coordinator integration checks.

## Change and coverage

`src/lib.rs::generic_type_scope(module, parameters)` creates the same nominal
qualification scope used by syntax canonicalization: a declaration's generic
parameter names exclude matching local records **and** variants. Public record
fields and variant case payloads use this scope when building
`ImportedSignatures`. Recursive qualification preserves binders inside
`Option<Result<List<Array<T>>, String>>`, while explicit `app.left.T` and
unshadowed `Tag` still retain their nominal module identities.

The change preserves all shared representations, declaration/source spans,
generic constraints, specialization keys, effect evidence, source hashes and
`verify_unchanged` admission. No dependencies, public schemas or other crates
change.

`tests/binders.rs` adds eight regressions using the three committed
`fixtures/binder-{record,variant,nested}` packages. They cover same-named local
record/variant binders (including a record binder shadowing a variant and the
reverse), nested containers, qualified/unshadowed nominal types, distinct modules
with colliding generic names, incorrect argument types, generic constraints,
private types, changed source hashes, stale snapshots and dishonest imported
effects. A changed generic dependency must invalidate the old snapshot with
`CGP018`; a newly loaded dependency with an undeclared `arg_count()` effect must
reject with `CGT012`.

The new tests were also run with only `src/lib.rs` restored temporarily to the
reviewed version, then the repaired source was restored in a `finally` block.
That run fails four tests (exit 101); the repaired run passes all eight.
`binder-evidence/before-repair.log` records the regression failures.

## Verification

Commands run from `/home/john-riley/.codex/worktrees/b9e0/lang`:

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-package-binders"
export TMPDIR="$PWD/target/tmp-package-binders"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"

cargo test -p cgx2-package --all-features --offline --locked
# PASS: 35 tests, 0 failed, 0 ignored, 0 filtered.
cargo clippy -p cgx2-package --all-targets --all-features --offline --locked -- -D warnings
# PASS.
cargo fmt -p cgx2-package -- --check
# PASS (after owned cargo fmt).
git diff --check
# PASS.
cargo build --offline --locked -p cgx2-cli -p cgx2c
# PASS; private-target cgx2 and cgx2c used for actual product checks.
python3 crates/cgx2-package/scripts/verify_binders.py \
  --binary-dir "$CARGO_TARGET_DIR/debug" \
  --output "$PWD/crates/cgx2-package/binder-evidence/report.json"
# PASS: 88 checks, 0 failures.
```

The product runner executes from a temporary directory under `TMPDIR`, using
absolute package/binary/output paths. It exercises the three owned packages and
the original reviewed `plans/release-completion/s2b-review/evidence/binder-package`.
Each checks successfully and returns exactly `42\n` through `cgx2 run`, actual
native debug and release executables, and actual emitted Python zipapps. Native
build metadata is recorded separately from executed program output.

The 88 checks comprise 32 positive product checks and 56 negative rejection and
artifact preservation checks. Eight package mutations cover record/variant and
nested argument mismatches, qualified nominal mismatches, module identity,
missing generic constraints, private records and hidden imported effects.
Each must reject on check, native build, Rust emission and Python emission;
each rejected build/emit must retain the preexisting artifact bytes.
`binder-evidence/report.json` records every command, working directory, exit
status, output, expectation and source/implementation SHA256. It is real product
evidence; no mocks, performance measurements or release claim are involved.

The first runner attempt incorrectly expected empty native-build stdout;
successful builds print provenance metadata. The runner now ignores build
stdout while checking its exit status and separately executing every resulting
binary against the exact expected output. The final 88-check run followed
formatting and a fresh CLI/compiler build. Earlier harness failures are not
counted as product failures or substituted for the final run.

## Independent failures and limits

`binder-evidence/other-lanes.json` captures these independent review failures
again using the repaired package crate and freshly built CLI. They are failures,
excluded from the 88 owned passing checks:

| Review source | Actual observation | Required other owner repair |
| --- | --- | --- |
| `constructor-shadow.cg` | `cgx2 check` exits 1 with `CGT005`/`CGT001` for a valid record named `length` | Semantic constructor precedence |
| `binder-constructor.cg` | `cgx2 check` exits 0 for an invalid attempt to construct the generic binder `T` | Semantic binder-aware constructor admission |
| `local-option.cg` | Native build exits 1 with `CGXC005`; rustc reports undeclared concrete `Option<i64>` | Rust concrete declaration discovery |

The remaining reviewed blockers are retained from the shared review evidence,
not rerun or claimed repaired here: unsupported string escapes panic during Rust
emission and are interpreted differently by Python; package interpreter output
exceeds its 1 MiB limit; public schema-7 API metadata exposes private
instantiations. See `s2b-review/README.md`, `evidence/report.json`,
`evidence/additional-report.json` and `evidence/native-local-option.json` under
`plans/release-completion/`. No sema, emitter, interpreter or API implementation
was edited in this lane.

Full combined workspace tests/Clippy/format/build, the foundation 159-product
runner, all original review repros, documentation/benchmark/readiness checks and
integration acceptance remain the coordinator's responsibility. No shared core
contract, release gate or source admission rule is relaxed. No main merge,
follow-on task, external services, package installation or host policy changes
were performed. The preexisting untracked `.serena/` directory remains untouched
and outside this commit.
