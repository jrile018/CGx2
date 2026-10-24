# S2b semantic-resolution repair

Base: `b595724e1b086c604220cc5b165c141e65a0531f`.
Branch: `riley/s2b-semantic-resolution` in the attached `0e24/lang` worktree.
Only `crates/cgx2-sema/**` is part of this delivery. The foundation remains
repair-required: the owned checker repairs pass, but standalone intrinsic-named
record native acceptance needs the Rust emitter owner as described below.

## Implementation and namespace choice

`src/generic.rs` now uses the same private `Checker::record_constructor(path)`
predicate for source call resolution and for deciding whether a data intrinsic
may be selected. Local, defining-module-qualified, public imported and private
imported record identities take the nominal checking path before intrinsic
dispatch. Existing import/privacy checks still apply. All ten names are covered:
`length`, `show`, `parse_i64`, `array`, `list`, `get`, `append`, `concat`,
`updated`, `slice`.

Function type binders continue to shadow unqualified nominal heads, as they
already do in type annotations. `infer_aggregate(syntax, case, args, hint, span)`
rejects a head in `self.binders` with located `CGT002: type parameter T cannot be
used as a constructor`, even if the result is discarded or the generic body has
no concrete caller. This covers record and variant construction, explicit type
arguments and nested generic bodies. A qualified head such as `model.T` remains
available beside the binder `T`. Checked imported effect evidence cannot be
published from a body with this error.

Aggregate declaration binders are distinct from the constructor head:
`record T<T>` and `variant T<T>` infer against the resolved nominal declaration
and parameter types, rather than accidentally resolving the head as their own
binder. Failed member/case lookup now emits `CGT002` when the lookup itself did
not emit a diagnostic; it cannot silently admit an untyped expression.

`src/foundation.rs` lowers a record call only when its callee identity agrees
with its checked record type. Payload naming is no longer the identity test.
Ordinary functions returning records remain calls. Existing nominal identities,
specialization keys, `Type`, `TypedModule`, `ResolvedCall`, imported summary
layouts, public signatures, dependency graph and schema versions are unchanged.

## Verification

Environment for every Cargo/product command:

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-semantic-resolution"
export TMPDIR="$PWD/target/tmp-semantic-resolution"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
```

Commands executed from the worktree:

```sh
cargo test -p cgx2-sema --all-features --offline --locked
cargo clippy -p cgx2-sema --all-targets --all-features --offline --locked -- -D warnings
cargo fmt -p cgx2-sema -- --check
rustfmt --edition 2021 --check crates/cgx2-sema/src/generic.rs crates/cgx2-sema/src/foundation.rs crates/cgx2-sema/tests/semantic_resolution.rs
git diff --check
cargo build -p cgx2-cli -p cgx2c --offline --locked
python3 crates/cgx2-sema/tests/semantic_resolution/run.py --binary target/release-semantic-resolution/debug/cgx2 --output crates/cgx2-sema/tests/semantic_resolution/product-report.json
```

- Owned package tests: **76 passed, 0 failed, 0 ignored**, including eight new
  semantic regression tests. Constructor tests cover 40 local/named-module,
  generic/non-generic combinations plus ten imported nominal names, import
  admission, checked evidence rejection, binder scopes and lowering identity.
- Owned all-target/all-feature Clippy with denied warnings, owned formatting,
  explicit formatting of included source files and diff checks: **pass**.
- Actual default-feature CLI/compiler build: **pass**.
- Product runner: **212 passed, 33 failed**, exits **1**, deliberately retaining
  cross-owner failures. Exact commands, outputs and diagnostics are committed in
  `tests/semantic_resolution/product-report.json`.

The product runner uses absolute source/binary/artifact paths and executes from
a private temporary directory, outside repository-relative assumptions. It runs
23 valid cases: the original constructor repro, a qualified constructor beside a
binder, an imported package and all ten intrinsic-named records in standalone
and named-module forms. All 23 pass source checking, interpreter execution and
emitted Python execution. The twelve qualified/package cases also execute native
debug, native release and standalone emitted Rust successfully. The imported
package returns `63`; other cases return `42`.

Four invalid cases (original binder repro, variant, nested generic body and
exported package function) reject `CGT002` on check/run and Rust/Python emission
and native debug/release build. All **16** forced artifact replacement attempts
preserve the original sentinel bytes. Source spans are asserted in unit tests.

Source/runtime subprocesses have a 512 MiB address-space cap, 15 CPU-second cap
and 20-second wall timeout. Native compilation retains the CPU/wall bounds but
does not impose that source-stress virtual-address cap on rustc/lld. The initial
run imposed it on both; lld failed creating threads even for valid qualified
programs. `compiler-limit-observation.json` preserves its totals (140/69) and a
full representative failure. A normal native qualified build subsequently
succeeded and executed `42`. These infrastructure failures are not counted as
product passes; the final report records the rerun and its exact policy.

## Required cross-owner repair

`crates/cgx2-emit-rust/src/data.rs`, `expression()`, selects a data intrinsic by
callee spelling unless `typed.functions` contains that name. It does not consult
the already checked record constructor identity. For the original repro, the
correctly typed record call is emitted as `cgx2_length(&v0)` instead of record
initialization; rustc rejects it with `E0277`/`E0308`. The same issue affects the
other nine intrinsic names. All **33** final failures are the original plus ten
standalone cases, each at native debug build, native release build and direct
`rustc --edition=2021` compilation of emitted Rust. Rust emission returning exit
zero alone is not counted as successful native execution.

The emitter owner must give the checked record constructor precedence over data
intrinsic dispatch, using existing expression-type/nominal evidence. Returning
to the normal record initializer emission path handles named fields without any
shared interface change. No emitter files were modified and no lowered identity
was renamed to hide this defect. Coordinator integration must rerun the owned
runner and require zero failures after this cross-owner repair.

## Bounded type expansion and limits

The unchanged 14-level `Pair` repro in `tests/semantic_resolution/type-expansion.cg`
rejects `CGT030: type expansion exceeds 4096 nodes or depth 64` under 512 MiB,
10 CPU seconds and 20 wall seconds. The existing bounded lowerer path remains
unchanged. No missing early allocation guard was demonstrated by this bounded
investigation; no deeper/unbounded stress was run and no new blocker is claimed.

No full workspace/integration suite, release timing, leak, foreign, GPU, solver,
external study or broader S2b/S3 acceptance claim is made. Combined workspace and
foundation product gates remain coordinator work. The untracked `.serena/`
directory was generated by the required Serena initialization and is excluded
from the commit. No main merge, other-lane write or follow-on task was performed.
