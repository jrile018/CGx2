# Native constructor dispatch repair

Bounded delivery from reviewed base `c2a039db3ea0b4e457a9f2e0c7626db77094d313`,
on `riley/native-constructors`. Only `crates/cgx2-emit-rust/**` changed.
Foundation acceptance and the wider release gates remain open.

## Implementation

`src/data.rs::expression` now defers to record initialization when the checked
expression type is `Type::Record(name)` and `name` equals the lowered callee's
complete nominal identity. Previously only the function table was consulted
before intrinsic spelling dispatch. Thus a valid `length(value: 42)` constructor
was emitted as `cgx2_length(&i64)` and failed native compilation.

The finite pipeline already checks source/typed-module equality, specializes
generics into concrete nominal identities, then checks the lowered module. This
repair consumes that existing evidence. It does not rename records, reserve
intrinsic names, change shared representations, or expand target admission.
Record arguments still use the existing source-order temporaries. Ordinary
functions returning records still follow function dispatch; genuine intrinsics
retain their existing checked helpers, budgets and diagnostics.

## Regressions

`tests/native_constructors.rs` adds five tests using real Rust compilation and
execution, both debug and `-O`, with `rustc --edition=2021` in a temporary working
directory outside repository assumptions. They cover:

- All ten names: `length`, `show`, `parse_i64`, `array`, `list`, `get`, `append`,
  `concat`, `updated`, `slice`; reordered named fields and exact aggregate results.
- Inferred generic constructors inside generic functions and nested list/call
  expressions, specialized to both `i64` and `Bool`; concrete
  defining-module-qualified and imported constructors.
- Ordinary generic functions returning records, including an intrinsic-named
  function, and all ten genuine intrinsics in an unshadowed program.
- Source argument order: division by zero must precede overflow despite reversed
  field declaration order. Single evaluation: one costly argument fits the
  checked work budget; a two-call control rejects `CGR010`. Both native profiles
  agree with the interpreter and produce no partial output on errors.

With only the four-line production change removed, these final regressions
produce **3 passes and 2 native-compilation failures** (exit 101). Restoring the
change produces **5 passes**. The original single-field constructor also failed
native compilation before the repair with E0277/E0308.

## Reproduction and verification

Run from the attached worktree; no installation or network access is needed:

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-native-constructors"
export TMPDIR="$PWD/target/tmp-native-constructors"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test -p cgx2-emit-rust --all-features --offline --locked
cargo clippy -p cgx2-emit-rust --all-targets --all-features --offline --locked -- -D warnings
cargo fmt --all --check
git diff --check
cargo build -p cgx2-cli -p cgx2c --offline --locked
python3 crates/cgx2-emit-rust/tests/native_constructors_product.py \
  --binary-dir "$CARGO_TARGET_DIR/debug" \
  --output-dir "$CARGO_TARGET_DIR/reproduced-products"
```

The owned wrapper invokes the three existing product runners verbatim, directing
their output into the requested directory. Shared historical reports are not
overwritten. Results against the private default CLI/compiler build:

| Gate | Result |
| --- | --- |
| Owned all-feature tests | 23 passed, 0 failed, 0 ignored; 8 suites including empty unit/doc suites |
| All-target/all-feature Clippy, `-D warnings` | Passed |
| Workspace format and diff checks | Passed |
| Offline locked CLI/compiler build | Passed |
| Semantic-resolution product runner | 278 passed, 0 failed |
| Foundation product runner | 159 passed, 0 failed |
| Emitter product runner | 104 passed, 0 failed |

The semantic runner's **33 previously failing native compile cases** (original
plus ten standalone names, each native debug/release and standalone Rust) all
pass, as do their 33 newly reachable execution checks. All earlier positives,
Python executions and binder-rejection artifact-preservation negatives pass.
The emitter runner separately compiles and executes standalone Rust 2021 with
and without `-O`, and executes generated Python, from an external working
directory. No performance measurements were performed.

Full product transcripts and owned verification logs are committed under
`native-constructors-evidence/`. `verification.json` records command outcomes,
tool versions and SHA256 hashes. Temporary generated executables remain in the
private target/temporary directories as controlled by the existing runners.

## Independent failures retained

The separate API visibility runner still reports **88/103 passed, 15 failed
execution cases**, from the independently assigned source-specialization
defects. Reproduce with a fresh output directory:

```sh
python3 crates/cgx2-api-index/tests/verify_visibility_cli.py \
  --binary "$CARGO_TARGET_DIR/debug/cgx2" \
  --output-dir "$CARGO_TARGET_DIR/reproduced-visibility"
```

`independent-visibility.json` is the runner's unmodified report; its historical
`base` field is part of that pre-existing runner, not this delivery's starting
base. Actual starting base and binary hash are recorded in `verification.json`.
`independent-qualified.json` additionally retains the original `out.main.Long`
probe: source checking passes, but execution rejects `CGR009` and both emissions
reject `CGT002` for doubled `out.main.out.main.Long`. The visibility fixtures also
retain internal public-specialization privacy failures (`CGT022`). These files
and their owner were not edited. No foundation/release completion, follow-on
work, or merge is claimed.
