# S2a delivery evidence — 2026-10-23

This is a bounded language/type integration delivery for coordinator review, not a
release decision. R01 remains subject to coordinator acceptance. R02–R15 remain
independently open. Grammar remains an implementation candidate pending R14.

## Implemented source semantics

- Concrete nominal variants with ordered, typed positional payloads; qualified
  zero/payload constructors; immutable record/variant nesting; public imports
  with colliding type/case names in separate modules.
- `match` statements with one scrutinee evaluation, selected-arm-only execution,
  scoped immutable bindings, shadowing/discards, complete coverage, consistent
  branch results, and duplicate/unreachable-arm diagnostics. A match can be a
  function/script tail; it is not a general expression usable after `let =`.
- Fixed typed `Option<T>` Some/None and `Result<T,E>` Ok/Err, including concrete
  nested applications, arguments, returns, imported identities and display.
  Constructors require explicit type arguments; there is no contextual inference.
- Interpreter and standalone/native-package Rust lowering, checked arithmetic,
  scalar runtime contracts called from match arms, and source byte spans.
- Existing pure scalar/record Python output retains its behavior, including
  defining-module-qualified record identity. New aggregate/deferred profiles
  reject precisely on the Python target; they are not approximated.

The source grammar and policy are in [S2A-CONTRACT.md](../S2A-CONTRACT.md).
Nested patterns and guards reject. Records remain immutable and retain their
one-to-eight-field parser limit. Field projection remains one segment at a time;
use a binding for a nested record before projecting again. Ordinary `if` keeps
its existing Unit statement behavior. Named scalar functions such as `match`
remain callable; the new words are contextual.

## Public interface migration

| Interface / file | Old → new representation |
| --- | --- |
| `cgx2-syntax/src/ast.rs` | `Module.variants`; `Record.type_parameters` and `Function.type_parameters`; `Variant { public, name, type_parameters, cases, span }`; ordered `VariantCase { name, payloads, span }` |
| Same | `TypeParameter { name, constraints: Vec<Path>, span }`; existing `TypeSyntax { path, arguments, span }` remains the type-application representation |
| Same | `StmtKind::Match { scrutinee, arms }`; `MatchArm { pattern, body, span }`; `Pattern::Wildcard(Span)` and `Pattern::Case { ty, case, bindings, span }` |
| Same | `ExprKind::Constructor { ty, case, arguments }`, `Collection(Vec<Expr>)`, `Index { collection, index }`; `StmtKind::LoopContract { decreases, expression }` |
| `cgx2-syntax/src/{visit,canonical}.rs` | Exhaustive `visit_expr` / `visit_stmts`, `s2a_feature_span`, and post-check `canonical_module` / `qualify_type`; all preserve source spans |
| `cgx2-sema/src/types.rs` | `Type::Record(String)` retained, with canonical defining-module identity even in standalone named modules; added `Type::Variant { name, arguments: Vec<Type> }` |
| `cgx2-sema/src/check.rs` | `ImportedSignatures.public_variants` / `private_variants`; exported record/variant payload syntax is qualified at its definition in `cgx2-package` |
| `cgx2-sema/src/lib.rs` | Added `resolve_type(module, imports, ty)` and `variant_cases(module, imports, ty)`; resolution does not expand aggregate fields |
| `cgx2-cli/src/eval.rs` | `Value::Variant { ty, case_index, case_name, payloads }`; `ty` is the concrete nominal `Type`; selection uses the defining type's case ordinal; `case_name` is display metadata |
| `cgx2-api-index/src/{index,render}.rs` | Schema 6 for records/new shapes: public qualified identities, nested `ApiType.arguments` with spans, type parameters/constraints, loop clauses, parameter/result/field types; records/variants sorted by identity, cases/payloads retain declaration order |
| Same | `build_syntax_index(module)` returns schema 6 with `semantics_status = "syntax-only"`, unchecked contracts/effects/capabilities and preserved foreign declarations; ordinary checked indexes still require a fresh matching semantic summary |
| `cgx2-cli/src/main.rs` | `cgx2 api --syntax-only [--json] FILE.cg`; package syntax-only indexing explicitly requires a source file, while ordinary checked package API remains supported |

Plain concrete `Reply.Value(...)` initially retains the existing Call AST form;
checked normalization makes its constructor explicit. An applied constructor
such as `Option<i64>.Some(...)` has its own source AST node. Constructors are
never recognized by runtime string-tag heuristics. A variant's payload values
have the case's statically checked types; no dynamic payload escape route is
admitted by the checker.

Canonical identity is the defining module path plus declaration name. Unnamed
standalone sources use local names in their single isolated module. Builtin
identities are `cgx2.core.Option` and `cgx2.core.Result`; arguments are part of
type equality. Imported types require the defining module's explicit source
import. No basename equivalence is used. Public signatures/payloads cannot expose
private nominal types, including through Option/Result; this is also enforced in
standalone checks. Private records are no longer emitted as public API entries.

Parser type nesting and semantic type-application nesting are bounded at 64.
Cycles through records, variants and Option/Result payloads reject with CGT030;
aggregate dependency paths of 64 active declarations also fail closed. This is a
finite-value profile, not a recursive/boxed representation. Collection payloads
and List-of-aggregate applications remain explicitly unsupported. Existing
interpreter step/depth bounds and native recursion rejection remain in effect.

## Deferred representation and consumer matrix

The parser/formatter/API retain `<T: Eq + Show>`, List/Array applications,
`[a, b]`, `values[index]`, and `invariant` / `decreases` expressions inside loop
suites. The canonical candidate places clauses at the start of a loop suite.
These source forms do not receive executable or proof semantics in S2a.

| Consumer/profile | Outcome |
| --- | --- |
| Core user generic declarations / named constraints | CGT032 at type parameter; substitution, inference and constraint solving are S2b |
| Collection literals / Array execution / checked indexing / annotated loops | CGT032 with source spans; no execution; S2b data semantics and S3-V proofs |
| Managed/owned memory script mixed with S2a aggregate/deferred profile | CGT023; existing memory/foreign routes retain their bounded profiles |
| Portable Python new profile, including package paths | CGE006 at the first new profile span; no target artifact publication |
| Notebook source, retained value, worker output and package admission | CGNK026 with offsets/source context; no variant execution or serialization added |
| Effect-index source (also syntax-only) and checked-package analysis | CGI006 admission diagnostic with source span/file; no partial subtree analysis claimed |
| Security source/package analysis | Propagates effect-index's explicit unsupported admission; no clean report for unsupported profile |
| Source verifier | CGV001, verdict `unknown`, proof state `unsupported`, bounded solver `unrun`; no solver start or proof claim |

All new source children are traversed by core call/effect/memory discovery, or
rejected before frozen downstream lowering/analysis. Concrete guarded rejection
arms cover shared enums; no unchecked wildcard admits a new form. Verifier core
limits (4096 nodes/parameters, depth 128, 1 MiB output and fail-closed emission)
were not changed.

S2b must implement user generic binding/substitution/inference/constraints;
collections/strings and checked indexing; ordinary owned/managed source
aggregates/lifetimes; capabilities and foreign/tensor integration; remaining
command closure; and Python variants/matching/Option/Result target semantics.
S3-N adopts these types for retained notebook execution; S3-V defines array and
annotated-loop proof obligations; S3-A adopts type-aware advisory analysis.
None of those completions is claimed here. No runtime/foreign crate, root Cargo
file, shared plan, design document or coordinator ledger was changed. No new
crate/dependency or coordinator-owned dependency change is required.

## Verification

Environment: Rust toolchain `1.98.1`, `CARGO_BUILD_JOBS=2`, private
`CARGO_TARGET_DIR=$PWD/target/release-s2a`, private
`TMPDIR=$PWD/target/tmp-s2a`, `LIBRARY_PATH=/tmp/cgx2-wave22-lib`.
Both private directories were created before checks. No installs/network calls,
performance measurements, solver acceptance or external studies were performed.

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-s2a"
export TMPDIR="$PWD/target/tmp-s2a"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test --workspace --all-features --offline
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo fmt --all --check
git diff --check
python3 crates/cgx2-syntax/s2a-evidence/verify_entrypoints.py \
  --bin-dir target/release-s2a/debug --out-dir target/s2a-entrypoints
```

- Final full workspace: **1059 passed, 0 failed, 6 ignored**, across 232 reported
  test/doc-test suites. Ignored tests require prepared pinned real LibTorch
  artifacts; they are not passing acceptance evidence. Earlier restricted runs
  hit worker lifecycle-guard execution denial; the successful full run used
  the required child-process/loopback execution access. No security policy was
  weakened to obtain it.
- Final all-target/all-feature workspace Clippy `-D warnings`: **pass**.
- Formatting and whitespace checks: **pass**.
- `cgx2c/tests/s2a_parity.rs`: **3 tests / 8 source-or-package scenarios**, each
  compiled and run in debug and release (**16 native binaries**). Covers nested
  aggregates, contract success/requires failure/ensures failure, overflow,
  nullary cases, None, matched Err, typed nested display, imported collisions,
  and the step-budget witness for one scrutinee evaluation. Source and native
  results/error classes agree; unselected failing arms do not execute.
- Product entry/package checks: **10/10 pass**. Exact commands/stdout/stderr and
  environment are retained in [entrypoints.json](entrypoints.json). Includes
  source/package interpreter entry, native source/package builds, no artifact
  on Python rejection, syntax-only schema/constraints/loop metadata, and
  structured semantic rejection of the deferred generic fixture.
- Workspace regressions include existing package/records/conditionals/contracts,
  portable Python records/scalars/contracts/ranges, memory and foreign routes,
  notebook consumers, verifier limits and source/SMT rejection tests.
- New negative tests cover coverage/duplicate/unreachable arms, payload count and
  type errors, unknown/private/imported and unrelated types, branch type
  mismatch, binding scope/duplicates, recursive shapes, generic arity, deferred
  constraints/data/loop clauses, nested patterns/guards, UTF-8 byte spans and
  bounded type nesting. Formatting tests compare span-erased AST structure and
  idempotent parse → format → parse results. API metadata is deterministic.

Local complete logs remain in ignored `target/s2a-{workspace-tests,clippy,fmt,
diff-check,parity-tests,entrypoints}.log`. Their exploratory failed runs were
replaced by the final checks above; no failure was relabeled as a pass.
