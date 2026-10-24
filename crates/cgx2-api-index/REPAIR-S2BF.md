# S2b foundation public-instantiation repair

Bounded API projection repair from reviewed base
`b595724e1b086c604220cc5b165c141e65a0531f`, on
`riley/s2bf-api-visibility`. All committed changes are under
`crates/cgx2-api-index/`. Foundation acceptance and other lanes remain pending.

## Implemented policy

`src/index.rs::build_index_checked` still freshly checks source/imports and
rejects invalid or mismatched typed modules before constructing an index. It
filters schema 7 instantiations using these rules:

1. The owner must be a public local checked function. The callee must be a
   public local checked function or a public imported function supplied in
   `ImportedSignatures.public_functions` with `public = true`.
2. Every resolved type argument, substituted parameter and substituted result
   must pass `public_instantiation_type`. Primitives are public; List/Array
   recurse into their element. Records and applied records require a public
   declaration with the exact canonical identity. Variants require the same,
   except core `cgx2.core.Option` and `cgx2.core.Result` are intrinsically public.
   Every nominal application's arguments are checked recursively, regardless of
   whether a binder is used by the resulting signature.
3. For a nominal identity defined locally, the local declaration's `public`
   flag decides visibility. Otherwise the corresponding `public_records` or
   `public_variants` imported entry must exist and have `public = true`.
   Unknown nominal identities are omitted conservatively.
4. A still-generic argument must exactly equal a checked type parameter of its
   public owner, including the scoped name and constraints. Nested public
   generic calls retain identities such as `List<api.nested::U>` and
   `Array<app.wrap::U>`; no textual prefix guessing or binder erasure is used.
5. Any failure omits the whole instantiation. Surviving entries retain the
   original complete types, canonical identities, spans and BTreeMap
   `(owner, start, end)` order. Visibility depends on declarations, not entry-point
   reachability. `TypedModule.calls` is neither changed nor filtered.

No shared type/signature/import layout, specialization key, admission contract,
dependency, renderer, schema version/selection, or effects/proof status changes.
Syntax-only schema 6 remains unchecked and has no instantiation metadata.

## Verification

Run from the attached worktree, creating both private directories first:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-api-visibility"
export TMPDIR="$PWD/target/tmp-api-visibility"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test -p cgx2-api-index --all-features --offline --locked
cargo clippy -p cgx2-api-index --all-targets --all-features --offline --locked -- -D warnings
cargo fmt -p cgx2-api-index -- --check
git diff --check
cargo build -p cgx2-cli -p cgx2c --offline --locked
```

All five checks/builds pass. **34 owned tests pass, zero fail or are ignored.**
The seven new tests cover private owners/callees, unnamed modules, private
record/variant/applied and container-nested arguments, complete public types,
imported public record/variant identities, scoped generic owners and constraints,
private/unknown instantiated parameter/result identities, deterministic rendering,
unchecked schema 6, invalid source and stale call evidence rejection. The
imported-signature defensive test supplies synthetic imported signatures; the
separate CLI package cases use actual checked package source.

With only the production API index implementation restored from the exact
reviewed base, **six new tests fail and one passes**. The repaired implementation
passes all seven. The base CLI reproduces all 15 target failures described below
with identical exit codes, stdout and stderr.

For product checks, resolve paths before changing directory; use fresh output
directories on reruns:

```sh
repair_root="$PWD"
cd /tmp
python3 "$repair_root/crates/cgx2-api-index/tests/verify_visibility_cli.py" \
  --binary "$CARGO_TARGET_DIR/debug/cgx2" \
  --output-dir "$CARGO_TARGET_DIR/product-fresh"
python3 "$repair_root/crates/cgx2-api-index/tests/verify_text_cli.py" \
  --binary "$CARGO_TARGET_DIR/debug/cgx2" \
  --output-dir "$CARGO_TARGET_DIR/s2a-fresh"
```

The visibility runner intentionally exits **1** while cross-lane failures remain:
**88/103 checks pass; 15 fail**. No known failure is counted as a pass. All new
API text/JSON checks pass: repeated deterministic checked source/package outputs,
the reviewed private owner/callee case, full nested public signatures and scoped
binders, constraints and checked/unchecked statuses. Private public-signature
source and private imported callee/type packages reject checked API text/JSON,
check/run, native build and Rust/Python emission; previously existing artifacts
remain byte-identical after every rejected forced emit/build.

`s2bf-public-values.cg` and `s2bf-public-package/` return **7** through the
interpreter, native debug/release builds, compiled emitted Rust and emitted
Python (package zipapp included). Both public execution cases pass all 11
checks each. The original S2a runner passes **33/33** CLI/source/target checks;
the original exact text/JSON golden tests also pass in the owned Rust suite.

Committed evidence:

- `evidence/s2bf-cli.json`: all repaired commands, cwd, exits, output, source and
  binary hashes, environment, assertions and failures.
- `evidence/s2bf-base-cli.json`: the same runner against the base API index.
- `evidence/s2bf-validation.json`: test/Clippy/build results, pre-repair failing
  test output and exact comparison of the baseline target failures.
- `evidence/s2bf-s2a-cli.json`: the existing 33-check product regression report.

Local detailed logs/generated programs remain in the ignored private target.

## Remaining source-execution failures outside exclusive ownership

All three fixtures pass source checking and API indexing. Each has five failed
execution/target attempts: interpreter, native debug build, native release build,
Python emission and Rust emission. Failed emissions/builds do not execute an
artifact. The baseline evidence reproduces these exact failures.

| Fixture | Failure | Required owning-path investigation |
| --- | --- | --- |
| `s2bf-visibility.cg` | Interpreter `CGR009`; build/emit `CGT022`: public signature exposes a private nominal type | `cgx2-sema` specialization rechecks a concrete public generic signature instantiated with a private local type |
| `s2bf-package/` | Same classes/message, at `src/main.cg:119..179` | Same specialization issue across checked package import/lowering |
| `s2bf-public.cg` | Interpreter `CGR009`; build/emit `CGT002`: unknown, unavailable or private type `public_api.public_api.Value` | `cgx2-sema` specialization/nominal qualification of a local generic variant payload |

These are preserved failed product checks, not repaired or waived acceptance.
No shared-owner changes were made. This delivery does not accept the whole
foundation, complete S2b, run integrated workspace/release gates, or establish
performance, solver, leak-enabled, external study or release results.
