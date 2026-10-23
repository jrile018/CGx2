# Wave 25 public record package APIs

`pub record Point:` exports an immutable nominal record. `record Point:` remains
private to its defining module. The record shape remains one to eight `i64`/`Bool`
fields. Import a module directly before using its qualified record annotation,
constructor, or a function signature carrying that defining module's type.

The readable two-module example is in
`crates/cgx2-cli/examples/public-record-api/`: `sample.points` defines `Point` and
`identity`; `sample.main` constructs fields in reverse declaration order, passes
through the public function, projects both fields, and returns `42`.

## Supported profile

- Acyclic, manifest-listed local modules; explicit imports and qualified record
  annotations/constructors. A public function can accept or return a public local
  record or an explicitly imported public record. Private local helpers can keep
  using private records within their defining module.
- Package `Type::Record(String)` identities are the complete defining module and
  record name, such as `demo.left.common.Pöint`. This identity is used by checked
  signatures/expressions, interpreter values, Rust types and Python classes.
  Legacy standalone checking retains its existing unqualified local identities.
- Linux CLI pure package execution through `cgx2 run PACKAGE`/`cgx2-native`, native
  `cgx2c build --package` in debug/release, emitted Rust, and standalone Python
  package files. Pure package interpretation requires named function modules and
  a zero-argument `main`. The existing pinned foreign routes remain separate.
- Pass-through, projection, construction, explicit returns and mandatory-else
  conditionals preserve checked arithmetic, selected-branch failures, source
  spans, statement/expression/frame budgets, and constructor argument order.
  Record storage/display uses declaration order. Existing scalar range-loop
  restrictions remain: loop calls must be pure local calls; imported calls in a
  loop are still rejected.
- Symbols encode complete identities. Same basename modules, same-shaped record
  names, Unicode, `self` fields and generated-looking source names are covered.
  Python imports the defining class rather than copying its layout. Public
  function parameters/results check exact class identity. Arithmetic failures
  retain `code` and `span` attributes and their original byte offsets.
- Package signature collection happens before execution/emission. Existing
  file hashes, canonical paths, stale snapshot checks and atomic output
  installation remain in force. The effect checker and call index retain
  constructor-argument dependencies; a known record constructor is pure.
  Security analysis remains conservative for qualified calls/field projections.

## Rejected profile and limits

Private record export/signature leaks, private or missing imported annotations
and constructors, missing defining imports, same-shaped nominal mismatches,
wrong nominal returns, ambiguous value/module roots, malformed imports and
cycles fail static checking. No wildcard imports, reexport aliases, registry
resolution, nested/mutable/generic/managed/foreign-layout records were added.

Notebook import admission rejects modules defining public records and rejects
record-typed imported functions before execution/replay. Diagnostics include the
import cell ID and byte span, plus the defining record span where available.
Even unused public records make that module unavailable to notebook imports in
this wave. Existing scalar imports, local records/functions, stale dependencies,
save/reopen, cancellation and transactional behavior retain their earlier
profile. General imported-record notebook execution remains unsupported.

Python exports canonical encoded class names in its generated files; source
annotations remain normal CGx2 qualified names. Generated Python is self-contained
but does not preserve Python traceback text or provide a sandbox for arbitrary
host reflection. This deliverable makes no new performance, memory-conformance,
foreign-layout, GPU, solver-proof or platform-portability claim.

## Implementation and validation evidence

Syntax changes only add `Record.public`; `Function`, `StmtKind::If` and scalar
`ExprKind` interfaces are unchanged. `ImportedSignatures.public_records` maps
canonical names to record definitions and `private_records` tracks inaccessible
names. Package signature collection uses
`collect_public_signatures_with_imports(module, imported)`; its older scalar
entrypoint remains available. Private module records cannot cross a public API.

The CLI lowers checked names internally while retaining original byte spans and
maps failures to the defining function's source file. Rust uses injective symbol
encoding. Python uses source-inexpressible internal import/private names before
its existing injective name encoder; emitted classes stay in their defining
modules. An atomic counter also prevents temporary test-directory collisions on
hosts with coarse wall-clock resolution.

All validation uses Rust/Cargo 1.98.1 and these private paths:

```sh
export CARGO_TARGET_DIR="$PWD/target/wave25-public"
export TMPDIR="$PWD/target/wave25-public-tmp"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
```

`/tmp` initially refused compiler metadata with a quota error; moving both
artifact directories into this worktree resolved it. The all-feature bridge tests
opt into the existing linker-name workaround via
`LIBRARY_PATH=/tmp/cgx2-wave22-lib`. Default native builds do not require it.
Loopback HTTP tests require socket permission outside the default sandbox.

Final results (2026-10-23):

| Command | Result |
| --- | --- |
| `LIBRARY_PATH=/tmp/cgx2-wave22-lib cargo +1.98.1 test --offline --workspace --all-features` | Passed: 766 tests, 0 failed, 5 ignored, 187 suite/doc-test result groups. Includes every owned suite and all 20 notebook HTTP tests; run with loopback socket permission. |
| `cargo +1.98.1 fmt --all -- --check` | Passed. |
| `cargo +1.98.1 clippy --offline --all-targets --all-features` with the owned package flags below and `-- -D warnings` | Passed. |
| `cargo +1.98.1 test --offline -p cgx2-cli -p cgx2-emit-rust -p cgx2-emit-python --test package_records` | Passed; library/CLI interpreter, Rust debug/release, Python, Unicode/same-basename collisions, nominal checks, first failure and byte spans are also covered in the final combined run. |
| `cargo +1.98.1 test --offline -p cgx2c --test record_parity` | Passed; includes actual public-record debug/release commands and failed private-record emission preserving a previous output. |
| `cargo +1.98.1 test --offline -p cgx2-package -p cgx2-sema -p cgx2-notebook-kernel` | Passed; direct imports, public imported constructors, private signature/type/constructor rejection, wrong nominal arguments/results, effect/capability dependencies, notebook admission, and existing retained-state behavior are also covered in the final combined run. |
| `cargo +1.98.1 build --offline -p cgx2-cli -p cgx2c --bins` without `LIBRARY_PATH` | Passed. |
| `target/wave25-public/debug/cgx2 check crates/cgx2-cli/examples/public-record-api` | Passed. |
| `target/wave25-public/debug/cgx2 run crates/cgx2-cli/examples/public-record-api` | Passed, stdout `42`. |
| `target/wave25-public/debug/cgx2c build --package crates/cgx2-cli/examples/public-record-api --out target/wave25-public-tmp/public-api-native --release`, followed by the binary | Passed, stdout `42`. |
| `readelf -d` on default `cgx2-native`, `cgx2c`, and the generated native example | No `libpython` dependency in any of the three. |
| `git diff --check` and ownership/frozen-path audit | Passed: all 41 changed files are owned; root Cargo files, shared docs/plans, verifier crates, benchmark compiler pins and foreign/runtime/bridge sources are unchanged. |

The exact owned Clippy package flags were:

```text
-p cgx2-syntax -p cgx2-sema -p cgx2-package -p cgx2-cli -p cgx2c
-p cgx2-emit-rust -p cgx2-emit-python -p cgx2-effect-index -p cgx2-security
-p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel
-p cgx2-notebook-cli -p cgx2-notebook-ui
```

The five ignored tests require external reviewed/pinned real LibTorch packages:
`real_pinned_libtorch_when_installed`, `real_pinned_matrix_when_installed`,
`real_pinned_float32_reference_and_cycles`, `real_pinned_matrix_element_boundaries`
and `real_libtorch_cpu_when_installed`. They are not record-profile evidence.
No benchmark/compiler pins or broad release gates were changed.

The first combined attempts exposed the default sandbox socket denial, a
coarse-clock temporary-directory collision in an owned Python test fixture, and
an effect-classification gap for qualified constructors. The final run above
passes after the fixture and constructor fixes and with HTTP socket permission.
Raw local logs remain in the ignored `target/wave25-public-tmp/` directory.

## Changed files


```text
crates/cgx2-cli/examples/public-record-api/cgx2.toml
crates/cgx2-cli/examples/public-record-api/src/main.cg
crates/cgx2-cli/examples/public-record-api/src/points.cg
crates/cgx2-cli/src/bin/cgx2-native.rs
crates/cgx2-cli/src/eval.rs
crates/cgx2-cli/src/lib.rs
crates/cgx2-cli/src/main.rs
crates/cgx2-cli/src/package_eval.rs
crates/cgx2-cli/tests/package_records.rs
crates/cgx2-effect-index/src/lib.rs
crates/cgx2-emit-python/README.md
crates/cgx2-emit-python/src/emit.rs
crates/cgx2-emit-python/src/package.rs
crates/cgx2-emit-python/tests/package.rs
crates/cgx2-emit-python/tests/package_records.rs
crates/cgx2-emit-rust/README.md
crates/cgx2-emit-rust/src/lib.rs
crates/cgx2-emit-rust/tests/package_records.rs
crates/cgx2-notebook-kernel/src/project.rs
crates/cgx2-notebook-kernel/tests/pure_imports.rs
crates/cgx2-package/README.md
crates/cgx2-package/docs/wave25-public-record-apis.md
crates/cgx2-package/fixtures/public-record-collisions/cgx2.toml
crates/cgx2-package/fixtures/public-record-collisions/src/left.cg
crates/cgx2-package/fixtures/public-record-collisions/src/main.cg
crates/cgx2-package/fixtures/public-record-collisions/src/right.cg
crates/cgx2-package/fixtures/public-records/cgx2.toml
crates/cgx2-package/fixtures/public-records/src/geometry.cg
crates/cgx2-package/fixtures/public-records/src/main.cg
crates/cgx2-package/src/lib.rs
crates/cgx2-package/tests/package.rs
crates/cgx2-sema/src/check.rs
crates/cgx2-sema/src/effects.rs
crates/cgx2-sema/src/lib.rs
crates/cgx2-sema/src/types.rs
crates/cgx2-sema/tests/records.rs
crates/cgx2-syntax/src/ast.rs
crates/cgx2-syntax/src/format.rs
crates/cgx2-syntax/src/parser.rs
crates/cgx2-syntax/tests/records.rs
crates/cgx2c/tests/record_parity.rs
```
