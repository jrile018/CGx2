# Wave 28: checked imported effects and capabilities

The local-package loader now checks bodies in dependency-first order and supplies
verified public function effects to callers. An explicitly pure wrapper around a
checked pure import works. Effects and capabilities from imported bodies and
local helpers propagate transitively, including `ReadArgs`/`ArgsRead`, `Foreign`
and its distinct `ForeignCall`/`native` capabilities, `Mutate`, and `Unknown`.
Existing memory-script import restrictions still reject allocation/collection
routes at the package boundary. No runtime, target, verifier or notebook profile
has been widened.

Implementation starts at shared revision
`d82aa8583a922d83f92a95cbe8462ef890e44c1e`. Changes are confined to the assigned
sema/package/API-index/CLI/emitter/compiler crates. No root Cargo files, shared
plans or notebook/advisory sources were changed.

## Evidence and freshness

`ImportedSignatures::checked_effects` defaults to empty opaque
`CheckedImportedEffects`. `check_and_record_module(&mut self, module: &Module)`
checks the supplied body using `check_module_with_imports`, then publishes only
public functions whose fully qualified identities/signatures match the supplied
declarations. It removes older evidence before a refresh, including a failed
refresh. Unnamed modules still check without publishing import evidence.

Each immutable summary retains its defining syntax, body-derived `EffectSummary`,
a complete declaration snapshot, and references to already checked dependencies.
Replacing/removing dependency evidence or changing declarations makes dependent
summaries unavailable until checked again. Diamond validation memoizes checked
module identities and has a conservative depth bound of 256. Summaries are owned
by the checking environment and never stored in a global cache.

The package loader collects declarations and validates the import DAG before
checking bodies. It checks in dependency-first order but preserves manifest order
in `CheckedPackage::modules()` and the existing sorted public index order.
Missing, private, unimported and unresolved calls retain their diagnostics and
conservative effects. Constructor arguments remain visited separately from the
constructor call. Calls in both conditional branches and empty range bodies and
bounds remain visible. Local recursive inference retains its existing bounded
fixed point. Local assignment remains `Mutate`.

Public annotations remain upper bounds: the API index reports declared names
with status `explicit`; unannotated APIs report body-derived names with status
`inferred`. Calls use body-derived effects, as local calls already did. A broad
annotation on an otherwise pure body is not silently removed from the public
index. `build_index_with_imports` still requires equality with a fresh semantic
check; changed signatures, effects or incomplete import evidence cannot reuse a
stale incompatible typed index. `TypedModule`, `EffectSummary`, existing checker
signatures and checked-package accessors are unchanged.

Package manifest/source/canonical-path freshness checks remain authoritative for
filesystem edits. The sealed semantic evidence does not replace
`CheckedPackage::verify_unchanged()`. Contract and range pure-call admission still
requires the existing local purity/termination checks; a resolved imported effect
set supplies no new contract proof. Signatures-only and standalone advisory
checks remain conservative and can still report imported calls as `Unknown`.

## Example and coverage

[`../fixtures/checked-effects`](../fixtures/checked-effects) contains a five-module
chain/diamond with explicit `effects()`/`caps()` wrappers, identical function
basenames in separate modules, public nominal records and constructors. Its
manifest lists callers first to exercise dependency ordering. All supported
execution/emission routes return `42`. Replacing the base arithmetic with
`x / 0` preserves checking and produces `CGR006` through the interpreter, emitted
Rust/Python and native debug/release. Replacing it with `arg_count()` dishonestly
inside `effects()`/`caps()` rejects before replacing existing artifacts.

Tests additionally cover transitive required capabilities, declaration-only and
partially checked environments, failed dependency refresh, declaration changes,
removed/replaced evidence, stale typed indexes, Unicode identities, private and
missing targets, effectful constructor arguments, inactive branches and empty
loops, and local pure/effectful recursion. Python/native foreign declarations are
checked without opening libraries or executing foreign calls. A schema-2 lock
path is used for the static native declaration test; no binding proof or new
foreign execution route is claimed.

## Verification (Rust 1.98.1, offline)

Commands were run from this attached worktree. Build and temporary outputs were
private to it:

```sh
export CARGO_TARGET_DIR="$PWD/target/wave28/build"
export TMPDIR="$PWD/target/wave28/tmp"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"

cargo +1.98.1 test --offline \
  -p cgx2-sema -p cgx2-package -p cgx2-api-index -p cgx2-cli \
  -p cgx2-emit-rust -p cgx2-emit-python -p cgx2c --all-features
# PASS: 291 tests, zero failures/ignored/filtered tests.

cargo +1.98.1 clippy --offline \
  -p cgx2-sema -p cgx2-package -p cgx2-api-index -p cgx2-cli \
  -p cgx2-emit-rust -p cgx2-emit-python -p cgx2c \
  --all-targets --all-features -- -D warnings
# PASS.

cargo +1.98.1 fmt --all -- --check
# PASS.
git diff --check
# PASS.

cargo +1.98.1 test --offline \
  -p cgx2-notebook-format -p cgx2-notebook-kernel -p cgx2-notebook-cli \
  -p cgx2-notebook-session -p cgx2-effect-index -p cgx2-security \
  -- --skip rejects_nonpublic_effectful_duplicate_and_transitive_calls
# PASS: 183 tests; one test filtered, explained below.

cargo +1.98.1 test --offline -p cgx2-notebook-ui --test http
# PASS: 22 tests. Requires loopback socket access outside the restricted sandbox.
```

The first HTTP attempt inside the sandbox failed to bind its local server with
`Operation not permitted`; rerunning with loopback permission passed. The first
permission review timed out; the permitted retry succeeded.

The unfiltered notebook regression attempt exposes one obsolete expectation in
`crates/cgx2-notebook-kernel/tests/pure_imports.rs`,
`rejects_nonpublic_effectful_duplicate_and_transitive_calls`, at the transitive
pure-wrapper case around line 231. It expects `CGT012` for an explicit pure wrapper
around a checked pure dependency. This wave intentionally accepts that program;
`NotebookKernel::open_with_project` succeeds. The notebook-owned test must be
updated during integration. Its earlier private/effectful cases still execute
before that assertion; the remainder is excluded by the filtered run. No
notebook tests were edited in this lane.

A direct existing notebook CLI smoke also passed with the committed diamond:

```sh
cat > target/wave28/explicit-import.cgnb <<'NB'
#!cgnb 1
%% code id=imports
import demo.base
import demo.join
%% code id=result
let answer = join.value(20)
answer.value
NB
cargo +1.98.1 run --offline -p cgx2-notebook-cli -- run \
  target/wave28/explicit-import.cgnb \
  --project crates/cgx2-package/fixtures/checked-effects \
  --out target/wave28/explicit-import-executed.cgnb --force
# PASS: saved text/plain output 42 and package/source provenance.
```

Native profile/error/atomic-output checks are committed in
`crates/cgx2c/tests/checked_effects.rs`; interpreter/CLI and Python package checks
extend their owned `package_records.rs` suites. The native compiler test also
compiles emitted Rust independently. Python generation is deterministic and
retains stale-snapshot/output protection. Real foreign libraries, live solvers,
benchmark claims and release-level memory assurance are outside this deliverable.
