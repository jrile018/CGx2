# S2b source specialization repair

Reviewed starting base: `c2a039db3ea0b4e457a9f2e0c7626db77094d313`.
Branch: `riley/source-specialization`. Scope: the assigned source-specialization
repair only. Foundation/release acceptance remains open.

## Implementation

- `crates/cgx2-syntax/src/canonical.rs`: `qualify_type` and the record-call
  canonicalizer recognize a dotted nominal identity stored in one path segment
  as already qualified, like a split path. Recursive type arguments retain this
  property. This is post-admission qualification, not an access check; identities,
  spans, source binders and nominal distinctions are preserved.
- `crates/cgx2-sema/src/foundation.rs`: `lower_module(module, typed)` retains its
  full unchanged-source recheck before specialization. Declarations in its
  generated implementation module have `public = false`. This includes generic
  functions, records and variants, and monomorphic functions/aggregates whose
  concrete interfaces reference generated aggregates. Their original public
  declarations and checked API evidence remain on the input module/TypedModule.
  Concrete bodies still undergo normal type/effect checking. No CGT022 check,
  trust check, constraint, schema, specialization key or shared layout changed.
- Added seven semantic and three syntax regressions, source/package fixtures,
  and `tests/source_specialization/run.py`. Coverage includes dotted modules,
  single-segment/split identities, fields, payloads, nested containers, colliding
  module/type/binder names, used/unused parameters, public concrete interfaces,
  caller-private and defining-module-private generic arguments, private imported
  types, unknown types, stale imported signature evidence, dishonest effects and
  generated-body rechecking. Existing package snapshot/hash tests also pass.

Making only generic declarations internal would be insufficient: a checked
public monomorphic function or record can refer to a concrete public generic
aggregate. Its lowered interface must also remain internal; otherwise ordinary
public-signature rechecking treats the generated implementation as a source
export. Source privacy is checked before this boundary, including genuinely
private types in public function, record and variant declarations.

## Reproduction and results

The original base was built first in the private target. The unchanged visibility
runner produced **88/103 checks, 15 failed execution cases**. The original small
qualified fixture checked but run rejected `out.main.out.main.Long` (CGR009
wrapping the unresolved concrete type). Four new semantic regressions also
failed before the production patch; the negative source privacy/effects test
already passed. Raw before-results are in `tests/source_specialization/evidence/`.

Final verification uses Rust 1.98.1, offline/locked Cargo, two Cargo jobs and the
private directories below. The results are conformance checks, not timings.

| Check | Result |
| --- | --- |
| Owned and read-only package/API consumer Rust tests, all features | 195 passed, 0 failed, 0 ignored; 48 suites including doc suites |
| Owned all-target/all-feature Clippy, `-D warnings` | passed |
| Owned Cargo formatting and Git whitespace checks | passed |
| Default CLI build | passed |
| Owned product runner, external `/tmp` cwd | 207/207 passed |
| Unchanged API visibility runner | 118/118 passed, all 15 previous failures removed |
| Unchanged foundation product runner | 159/159 passed |
| Unchanged package binder runner | 88/88 passed |
| Unchanged output bounds runner, native compiler address space 4096 MiB | 31/31 passed, including qualified-standalone-admission |
| Unchanged semantic-resolution runner | 212 passed, 33 independent native-constructor failures |

The owned runner executes all five API visibility/public fixtures, the original
small qualified fixture, nested private function/record/variant specializations,
public concrete interfaces and a same-name collision fixture. Every positive
runs interpreted, native debug, native release, emitted standalone Rust compiled
with `rustc --edition=2021`, and emitted Python (including package zipapps).
Its subprocess cwd is an external `/tmp` directory; artifacts remain in the
private worktree target, so the temporary filesystem need not hold executables.
The exact qualified display is `[out.main.Long(x: é)]\n` on every route.

Metadata assertions from the original visibility runner are unchanged. Internal
ResolvedCall/private-type evidence is retained while checked public API output
omits private owners, callees and type arguments. The owned product runner also
rejects public signature/aggregate leaks, unknown/private imported type arguments
and dishonest local/imported generic effects before target artifacts, preserving
all 28 pre-existing sentinel artifacts for native debug/release, Rust and Python.
The output runner's qualified large standalone probe must reach CGR010 with no
stdout, rather than failing concrete type admission.

## Reproducible commands

Run from this attached worktree; no package installation or external service is
needed. All product runners use absolute source/binary/artifact paths internally.

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR=$PWD/target/release-source-specialization
export TMPDIR=$PWD/target/tmp-source-specialization
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo build --offline --locked -p cgx2-cli
cargo test --offline --locked -p cgx2-sema -p cgx2-syntax -p cgx2-package -p cgx2-api-index --all-features
cargo clippy --offline --locked -p cgx2-sema -p cgx2-syntax --all-targets --all-features -- -D warnings
cargo fmt --package cgx2-sema --package cgx2-syntax -- --check
git diff --check
python3 crates/cgx2-sema/tests/source_specialization/run.py --binary "$CARGO_TARGET_DIR/debug/cgx2" --output-dir "$TMPDIR/final-external"
python3 crates/cgx2-api-index/tests/verify_visibility_cli.py --binary "$CARGO_TARGET_DIR/debug/cgx2" --output-dir "$TMPDIR/final-visibility"
python3 crates/cgx2-syntax/s2b-evidence/run.py --binary-dir "$CARGO_TARGET_DIR/debug" --output "$TMPDIR/final-foundation.json"
python3 crates/cgx2-package/scripts/verify_binders.py --binary-dir "$CARGO_TARGET_DIR/debug" --output "$TMPDIR/final-binders.json"
python3 crates/cgx2-cli/scripts/verify_output_bounds.py --binary "$CARGO_TARGET_DIR/debug/cgx2" --output "$TMPDIR/final-output.json" --build-address-space-mib 4096
python3 crates/cgx2-sema/tests/semantic_resolution/run.py --binary "$CARGO_TARGET_DIR/debug/cgx2" --output "$TMPDIR/final-semantic-resolution.json"
```

The two `--output-dir` runners intentionally require a new directory for each
run. Use fresh names if repeating. The semantic-resolution command exits 1 for
the independently assigned failures below. Captured reports preserve every
actual subprocess command, stdout/stderr and results; the output runner records
stdout hashes/lengths because its boundary outputs are large. `validation.json`
records file/binary hashes and final counts. Final evidence is under
`crates/cgx2-sema/tests/source_specialization/evidence/`; generated executable,
Rust/Python and rejected sentinel artifacts remain under private `target/`.

## Infrastructure and independent limitations

One preliminary repaired visibility run was interrupted by the shared `/tmp`
user disk quota. Its traceback is preserved as `quota-interrupted.log`; it is
not counted as a pass. Only this lane's disposable `/tmp` artifact directories
were relocated into the private target and removed to recover space. Final
runs use the specified private TMPDIR; the owned runner uses `/tmp` only as an
empty execution cwd. No host settings, packages or other lane artifacts changed.

All **33** remaining semantic-resolution failures are the original plus ten
local intrinsic-spelling constructor fixtures, each failing native debug build,
native release build and standalone rustc. Every failure retains the runner's
cross-owner tag for `cgx2-emit-rust/src/data.rs` intrinsic dispatch; the original
failure contains E0277/E0308. The source, interpreted and Python routes, and
qualified constructor routes, pass. That emitter belongs to the concurrent
native-constructors lane and was not edited. This commit does not claim whole
foundation/S2b/release acceptance, external gates, a main merge or follow-on work.

Required Serena setup left untracked `.serena/`; it is excluded from the commit.
