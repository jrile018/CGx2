# S2b foundation emitted-targets repair

Bounded repair from exact reviewed base `b595724e1b086c604220cc5b165c141e65a0531f`.
This delivery requires coordinator review; it does not accept the foundation or
close R02/R10 or the release gates. Only the two assigned emitter crates changed.
No dependencies, shared interfaces, root Cargo files, or other lanes changed.

## Changes

- `src/variants.rs` discovers concrete nested Option/Result declarations from
  every `TypedModule.bodies` span map as well as the existing module span map,
  signatures, records and variant payloads. Specializations can reuse spans but
  have distinct local types. Recursive collection and BTreeMap ordering preserve
  complete, deterministic definitions without changing specialization metadata.
- Both finite emitters validate string literals with the exhaustive syntax
  visitors before lowering/output. Traversal includes nested statements,
  collections, indexes, constructor/call arguments, generic bodies, `requires`
  and `ensures`. Unsupported escapes return existing `CGRS003` / `CGE003`
  diagnostics with literal byte spans, before the Rust decoder can panic or
  Python can reinterpret the text. Package validation runs on each original
  checked module before merging, retaining package-relative defining file/spans.
- Python finite emission reuses its existing CGx2 decoder and `quote_python`.
  Allowed source escapes remain `n`, `r`, `t`, backslash and double quote.
  Unicode stays literal; x/u/U/octal and other host-only escapes are rejected.
- `../cgx2-emit-python/tests/s2b_repair.rs` executes interpreted, emitted Rust
  2021 debug/optimized and Python cases. It covers both specialization orders,
  local-only Option/Result, nested records/variants/collections, contract-only
  calls and generic contracts within current source admission. Six unsupported
  escape forms are tested in six source contexts, checking exact literal spans.
  A package regression verifies imported file/span and existing output survival.
- `../cgx2-emit-python/tests/s2b_product.py` runs the actual CLI from an external
  temporary working directory, using absolute source/binary paths. Owned fixtures
  are in that crate's `tests/fixtures/s2b-repair/`.

## Verification

All commands were run in this attached worktree, with:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-emitted-targets"
export TMPDIR="$PWD/target/tmp-emitted-targets"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"

cargo test -p cgx2-emit-rust -p cgx2-emit-python --all-features --offline --locked
cargo clippy -p cgx2-emit-rust -p cgx2-emit-python --all-targets --all-features --offline --locked -- -D warnings
cargo fmt -p cgx2-emit-rust -p cgx2-emit-python -- --check
git diff --check
cargo build -p cgx2-cli -p cgx2c --offline --locked
python3 crates/cgx2-emit-python/tests/s2b_product.py \
  --binary-dir "$CARGO_TARGET_DIR/debug" \
  --output "$CARGO_TARGET_DIR/product.json"
```

- Owned suites: **69 passed, 0 failed, 0 ignored**, 16 suites including doc tests.
- Owned all-target/all-feature Clippy, formatting and diff checks: passed.
- Offline locked product build: passed.
- Product runner: **104 checks passed**. Six positive cases cover the exact
  reviewed local-option repro, richer nested data in both call orders, supported
  escapes/Unicode, generic contracts and an imported generic package. Each runs
  source check/run, native debug/release build/run, emitted Rust compiled with
  `rustc --edition=2021` with and without `-O`, and Python (package `.pyz`).
- Four invalid-literal contexts still pass source checking and fail source run
  with `CGR009`. Rust/Python emission and native debug/release builds reject with
  `CGRS003` / `CGE003`, without panic, while `--force` leaves pre-existing artifact
  bytes intact. Imported invalid literals also preserve existing Rust/Python
  package output and identify the defining file.
- Before the final package regression was added, the first three new regression
  tests were also run with the five production source files temporarily restored
  from the exact base, then restored in a `finally` block. The baseline suite
  failed (exit 101): declaration discovery failed Rust compilation with missing
  Option declarations, and invalid literal emission panicked. Supported-string
  parity passed. This is recorded in `baseline-regressions.log`.

`S2BF-EVIDENCE.json` records tool versions, individual product outcomes, source
hashes and log SHA256 values. Full logs and the 104-command product transcript
remain in the private ignored `target/release-emitted-targets/` directory:
`baseline-regressions.log`, `tests.log`, `clippy.log`, `build.log`, `product.log`
and `product.json`. Generated sources/executables are temporary and removed by
the tests; the runner and committed fixtures reproduce them.

## Exact limitations

- Source checking still admits unsupported string escapes. The interpreter's
  existing rejection is `CGR009`; target admission uses its existing literal
  classes `CGRS003` / `CGE003`. No sema admission or diagnostic API changed.
- Existing `CGT014` admits contracts only with annotated i64/Bool parameters and
  results. A function with a T-typed parameter and contracts remains rejected.
  Generic contract coverage uses explicit type arguments, i64 parameters/results,
  and nested Option values inside contracts; it does not claim broader support.
- Validation covers all source bodies, including generic templates, before
  specialization; output is never returned for a source with an invalid literal.
- Other S2b review failures (semantic resolution, package binders, interpreter
  output and API visibility) are outside ownership and remain for their lanes.
  Combined workspace/product review, all remaining S2b integration, S3, and
  external release evidence remain coordinator work. No performance measurements,
  package installation, main merge or follow-on work was performed.
