# S2b interpreter output repair

Base: `b595724e1b086c604220cc5b165c141e65a0531f`. Owned paths: `crates/cgx2-cli/**`.
This is a bounded repair for coordinator review, not foundation or release acceptance.

## Behavior

`src/eval.rs::finalize_value(value, span)` now serves standalone and package
execution. It retains the existing finite-value checks (depth below 64, at most
65,537 nodes, sequence length at most 65,536, aggregate string bytes at most
1,048,576), then checks displayed output against the existing 1,048,576-byte cap.
The cap excludes the CLI's final newline; an exactly full successful value is
returned intact and the CLI writes 1,048,577 bytes including its newline.

`check_output_bound(value, span)` runs the existing `Value::Display` and nested
`Type::Display` formatters against a nonallocating `fmt::Write` byte budget. Each
write subtracts `text.len()` with checked arithmetic and stops immediately on
overflow. Nominal/type/field/case names, generic arguments, collection punctuation,
and actual decoded string UTF-8 bytes all count. No display-sized temporary
string is built for rejected values. Formatting and Unicode scalar operations
retain their existing semantics. `src/data_eval.rs` applies the same preflight
before `show` allocates its successful String.

Output failures remain `CGR010: output exceeds 1048576 bytes`; finite-value
failures retain their existing messages. Final failures use the whole entry
module's span, matching the standalone contract. `src/package_eval.rs` resets
the merged module span to the entry module rather than the first dependency;
ordinary expression failures retain their defining function's origin and span.

No shared API, schema, backend, dependency, lockfile, or other lane was changed.

## Reproduction and verification

Run from this attached worktree (product subprocesses themselves run from a
private temporary directory outside repository-relative execution assumptions):

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR=$PWD/target/release-interpreter-output
export TMPDIR=$PWD/target/tmp-interpreter-output
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test -p cgx2-cli --all-features --offline --locked
cargo clippy -p cgx2-cli --all-targets --all-features --offline --locked -- -D warnings
cargo fmt -p cgx2-cli -- --check
git diff --check
cargo build -p cgx2-cli -p cgx2c --offline --locked
python3 crates/cgx2-cli/scripts/verify_output_bounds.py \
  --binary "$CARGO_TARGET_DIR/debug/cgx2" \
  --output "$PWD/crates/cgx2-cli/evidence/s2b-output-repair.json"
python3 crates/cgx2-cli/scripts/verify_output_bounds.py \
  --binary "$CARGO_TARGET_DIR/debug/cgx2" --release-only \
  --build-address-space-mib 4096 \
  --output "$PWD/crates/cgx2-cli/evidence/s2b-output-release-recheck.json"
```

- Owned offline tests: **119 passed, 0 failed, 0 ignored** across 25 suites,
  including seven new tests in `src/output_tests.rs` and `tests/output_bounds.rs`.
- Owned all-target/all-feature offline locked Clippy with warnings denied: pass.
- Owned formatting, `git diff --check`, and default CLI/compiler build: pass.
- New tests cover below/at/above cap, long nominal/field/type names, nested
  record/variant/List/Array display, decoded Unicode and escaped strings,
  unchanged scalar length, finite node/depth/string limits, guarded `show`, both
  entrypoints, and human/JSON CLI diagnostics without partial stdout.
- `tests/fixtures/output-bound/package` copies the exact reviewed output repro;
  its source SHA256 is
  `f9c42ee3032cbb862f011d52b6d5188908683d8daadd9084d9a4a0ef3b300004`.
  Its former 1,242,001-byte output is rejected with `CGR010` and no stdout by
  the repaired package interpreter, admitted standalone equivalent, native debug
  executables, and emitted standalone/package Python.
- The normal capped product report records **26 passed, 3 failed**. The failures
  are two large release compilations under the 2 GiB compiler cap and the
  independent qualified-standalone admission failure described below. They are
  retained as failures; the runner exits 1. Nested Unicode product execution
  passes interpreter/native debug/native release/emitted Python parity. Rejected
  native build and Rust/Python emit preserve existing requested artifacts.
- The separate **4 GiB release recheck: 4 passed, 0 failed**, runner exit 0.
  Both reviewed-package and admitted standalone equivalents build in release
  mode and actually execute, rejecting with `CGR010` and zero stdout. Thus the
  output repro has interpreter/native debug/native release/Python error parity
  for both admitted source routes; the qualified-standalone probe stays failed.

## Limits and unresolved acceptance

Product executions have 512 MiB address-space, 15 CPU-second and 20-second wall
limits. Builds have a default 2 GiB address-space, 60 CPU-second and 120-second
wall limit; release rechecks use 4 GiB. Each subprocess has a 16 MiB file cap and
disabled core files; timeout cleanup kills its process group. These are safety
caps, not performance measurements.

On this host lld's default thread pool fails even a small native link under the
capped environment (`std::system_error: Resource temporarily unavailable`). The
runner uses `rustup which rustc` for the pinned compiler and a temporary wrapper
with `-Awarnings -Clink-arg=-Wl,--threads=2`. A small native build succeeds with
that wrapper, and the large debug repro then builds and executes. Warnings are
suppressed only for generated-code product compilation; owned Clippy still
denies warnings. Initial default-compiler attempts are kept in the private
`target/release-interpreter-output/product-initial.json` log. No host settings or
backend sources were changed.

The two large release compilations under the 2 GiB cap fail with `CGXC005`:
rustc aborts after `memory allocation of 24760 bytes failed`. Their release
executables were not run in that report. The separate 4 GiB recheck report
records successful compilation and execution of both release repros without
erasing these failures. Release parity therefore requires the larger compiler
address-space allowance on this host; executable limits remain 512 MiB.

Running the package's module-qualified entry `.cg` directly still fails before
output handling with `CGR009` at `653..1263`: unavailable/private type
`out.main.out.main.Long…`. Even the small program
`module out.main; record Long(x: String); fn main() -> List<Long>: [Long(x: "é")]`
(written using ordinary indentation/newlines) reproduces doubled qualification.
This is recorded as a failed independent admission probe, not a passing output
test. Standalone output regressions remove the module declaration; package
regressions retain it. This repair does not change semantic resolution or claim
that all original review blockers pass.

Existing one-million-step/work limits remain independent constraints: an output
below the display cap can still reject for evaluator work or finite data size.
The boundary tests use nominal display overhead to reach the exact output cap
without violating those limits. Native/Python rendering still uses their own
existing allocation behavior; bounded rendering was repaired only in the owned
interpreter. Full combined workspace/product gates and other review repairs
remain the coordinator's integration work.
