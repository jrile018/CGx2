# S2a package record lowering repair

Bounded native-records delivery, 2026-10-23, on
`riley/s2a-native-record-repair` in the attached `7f73/lang` worktree.
Attached repair base: `8902cbd92533b81e801bf2c5bea725f5508fe3e2`
(includes the staged S2a delivery and coordinator repair plans).

## Change and audit

`src/lib.rs::emit_package_rust` now takes record fields from the existing
`canonical[source.source()].records`, as the package's variant and function
emission already do. Record declaration names still receive their defining
module prefix. Previously only the record name was qualified, leaving local
nominal field references and their nested type arguments unqualified.

The existing syntax canonicalizer recursively qualifies local records and
variants in `TypeSyntax.arguments` and preserves explicitly qualified imported
paths. The emitter's existing `syntax_type`/`rust_type` mapping therefore now
receives the same complete identities for record fields, variant payloads,
constructors, patterns and checked signatures. No basename lookup or special
case for the coordinator's fixture was added. Public interfaces, semantic
admission, root Cargo files and dependencies are unchanged. Standalone Rust,
memory and foreign routing code is unchanged; Python files are read-only.

## Regression coverage

All fixtures are under `tests/fixtures/` in this crate:

| Fixture | Exercised behavior | Source / native debug / native release |
| --- | --- | --- |
| `s2a-local` | Record-of-variant, record-of-record, variant-of-record; local `Option<Flag>`, `Result<Point, Flag>`, `Option<Result<Point, Flag>>`, `Result<Option<Point>, Flag>` fields; payload and explicit nullary patterns | 42 / 42 / 42 |
| `s2a-imports` | Two defining modules each declare `Point`, `Flag`, `Holder`; local fields inside both imported holders; imported record/variant fields and nested applications with mixed defining modules in the entry's `Bundle` | 42 / 42 / 42 |
| Local fixture with alternate constructors | Nullary `Flag.Off`, `Option.None`, `Result.Err` and alternate nested branches | 15 / 15 / 15 |

`tests/s2a_record_identities.rs` asserts complete generated field type identities,
including both modules and both nested generic application orders. It also checks
variant payload identities, deterministic emission and absence of bare nominal
type names. This verifies emitted declarations independently of scalar output.

`../cgx2c/tests/s2a_package_record_parity.rs` checks source evaluation and runs
both compiled profiles. Its five negative mutations cover a colliding imported
record, a colliding record payload in `Result`, a colliding variant in `Option`,
a colliding variant in nested `Option<Result<...>>`, and an unknown field type.
Each fails source execution and both `emit-rust` and `build` with CGx2 type
diagnostics (`CGT...`), before Rust compilation. All ten target attempts preserve
an existing destination artifact even with `--force`.

The new positive compiler regression was also run against the original record
emission path: it failed (exit 101) with `CGXC005/E0425` for bare `Flag`, `Point`,
and the four incorrectly instantiated `Option`/`Result` field types. Restoring
the fix makes that same test pass. The pre-existing single-field-segment access
profile is respected by binding intermediate records to locals in fixtures.

## Verification

Commands ran offline with this environment (no dependencies installed):

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR=$PWD/target/release-s2a-native-records
export TMPDIR=$PWD/target/tmp-s2a-native-records
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"

cargo test --offline -p cgx2-emit-rust --all-features
cargo test --offline -p cgx2c --test s2a_package_record_parity --test s2a_parity --test record_parity
cargo clippy --offline -p cgx2-emit-rust -p cgx2c --all-targets --all-features -- -D warnings
cargo test --offline -p cgx2-emit-python --all-features
rustfmt --edition 2024 --check crates/cgx2-emit-rust/src/lib.rs crates/cgx2-emit-rust/tests/s2a_record_identities.rs crates/cgx2c/tests/s2a_package_record_parity.rs
git diff --check
```

- Rust emitter: **17 passed, 0 failed, 0 ignored** (9 emission, 5 memory,
  2 existing package-record, 1 new identity assertion).
- Compiler targets: **8 passed, 0 failed, 0 ignored** (3 new package aggregate,
  3 existing S2a, 2 existing record parity tests). Existing tests were not edited.
- Python emitter, read-only regression: **46 passed, 0 failed, 0 ignored**,
  including existing scalar, record/package, loop and differential tests.
- All-target/all-feature Clippy for the Rust emitter and compiler: pass with
  `-D warnings`.
- Owned Rust formatting check and whitespace check: pass.

Local ignored logs are in `target/release-s2a-native-records/`:
`baseline-red.log`, `emitter-tests.log`, `compiler-regressions.log`, `clippy.log`,
and `python-regressions.log`. Successful final test/build commands exited 0;
the deliberate baseline-red reproduction exited 101.

## Limits and handoff

These are finite source/native conformance checks for the assigned lowering
repair. They do not close R01 or approve S2b/S3, establish a language-wide safety
proof, or supply external acceptance, leak-enabled memory assurance, live
solver, GPU, performance or human/model study evidence. No external acceptance
was attempted. No shared interface changes or coordinator Cargo reconciliation
are needed. Other repair lanes and shared ledgers were not edited. Commit this
delivery and stop for coordinator/user review.
