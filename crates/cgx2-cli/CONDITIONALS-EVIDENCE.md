# Wave 24 Boolean conditional statements

Base: `425cb0a5467d98b2cf9565be19067b2f39f23fff`.
Implementation and verification use the attached `d350/lang` worktree and Rust
1.98.1. No root Cargo files, shared design/roadmap/ledger files, runtime,
foreign/bridge, binding, LibTorch, verify-core, or benchmark sources were edited.

## Supported behavior

`if <Bool expression>:` requires nonempty, indentation-delimited then and `else:`
blocks. Nesting is supported. Conditions run once; only the selected branch runs.
Branch bindings are lexical, immutable bindings stay immutable, and assignments
to existing mutable outer scalars persist. Branch tails are discarded and the
statement yields Unit. Unit functions can fall through. A final conditional in a
non-Unit function needs explicit returns on every path; a partial return needs a
later ordinary tail expression or return. Private function return inference joins
explicit returns with the fallthrough type. Static checks still visit unreachable
source. Rust emission stops emitting after a definite return, while static
validation retains the entire body.

The interpreter, emitted Rust (including native debug/release), and emitted Python
support existing scalar and immutable local-record function profiles. Standalone
scripts support conditionals with the existing restriction against top-level
returns. Local acyclic packages retain scalar imported/exported signatures.
Foreign Python calls continue to use the explicit isolated worker in the CLI.
A worker-marker regression observes exactly one guard call and no worker startup
for an inactive branch. All paths retain existing source-order checked arithmetic,
UTF-8 byte spans, source diagnostics, and atomic output installation.

Both branches and the guard contribute static types, calls, effects and
capabilities. Taint analysis joins outer states conservatively, restores shadowed
outer values, removes branch locals, and retains conditional return taint across
later fallthrough expressions or returns.

Pure notebook function bodies support the same existing scalar/local-record
profile. Branch-local dependency sets are separate. Conditions, annotations and
both branches contribute dependencies; changing a function/import used only in
an inactive branch stales users. Existing cell values remain usable as ordinary
function arguments. Retained values are published transactionally; tests cover
failure recovery, worker and in-process execution, save/reopen, Restart and Run
All, and HTTP stale/run/save/restart. Replay admission counts both branches
conservatively; existing runtime step/depth, retained-value, timeout and
cancellation limits remain in force.

Examples:

- `crates/cgx2-cli/examples/conditionals.cg`
- `crates/cgx2-notebook-cli/examples/conditionals.cgnb`

## Explicit boundaries

- No conditional expression, `elif`, truthiness, pattern matching, or new
  comparison/logical operators.
- Conditional source memory programs reject with CGM001 before execution or
  emission, including memory in an inactive branch/guard or a conditional nested
  under `no_gc`. Ownership-state joins are not implemented.
- Conditional proof bodies report CGV001 unknown/unsupported at the conditional;
  no selected-branch proof or new solver claim is made.
- Python foreign bindings remain unsupported by Rust/Python emitters under their
  existing foreign profile. Native-C and tensor adapter entry bodies retain
  their existing restricted expression profile and reject conditionals.
- Package record import/export remains unsupported. Pure notebooks still reject
  top-level conditionals, captured cell values, mutation/effects, foreign calls,
  contracts and previously unsupported imports. Direct pure scalar imports work;
  transitive notebook module calls retain their existing rejection boundary.
- Rust source emission retains its existing recursive-call rejection. Ordinary
  interpreter execution retains the 1,000,000-step and 64-call-depth limits.
- Security output remains bounded advisory analysis. No real LibTorch, live trusted
  solver, performance, or release-level memory claim is added.

## AST consumer audit

| Consumer | Conditional handling |
| --- | --- |
| `cgx2-syntax/src/{ast,lexer,parser,format}.rs` | If AST/keywords, mandatory else, source spans, canonical nested formatting; syntax span-normalization test walker extended |
| `cgx2-sema/src/check.rs` | Guard Bool check, independent lexical scopes, call ordering, definite returns, private return inference, range-loop restrictions recurse |
| `cgx2-sema/src/effects.rs` | Guard and both branches contribute effects/capabilities |
| `cgx2-sema/src/memory.rs` | Recursive memory detection and explicit pre-lowering rejection; extraction/lowering also reject |
| `cgx2-sema/src/tensor.rs` | Existing single-expression adapter gate rejects a conditional body |
| `cgx2-cli/src/eval.rs` | One guard evaluation, selected scope, outer assignments, return propagation, recursive unsupported-profile checks |
| `cgx2-cli/src/{memory,native_eval}.rs` | Explicit conditional rejection in restricted lowerer/foreign entry |
| `cgx2-emit-rust/src/lib.rs` | Recursive validation/cycle discovery, scoped branch emission, discarded branch tails, definite-return emission |
| `cgx2-emit-rust/src/memory.rs` | Recursive profile detection and explicit rejection |
| `cgx2-emit-python/src/emit.rs` | Recursive target/record validation, scope/name collection and renaming, selected-branch emission |
| `cgx2-emit-python/src/package.rs` | Branch-name collision collection and guard/both-branch import/call lowering |
| `cgx2-effect-index/src/lib.rs` | Guard/both-branch calls/effects with independent local binding sets |
| `cgx2-security/src/lib.rs` | Recursive assignment policy, conservative taint joins and branch-local restoration |
| `cgx2-package/src/lib.rs` | Guard/both-branch memory and imported-call discovery |
| `cgx2-verify-cli/src/source.rs` | Explicit unsupported extraction with conditional span; existing source audit fallback rejects unsupported bodies |
| `cgx2-notebook-kernel/src/lib.rs` | Both-branch calls/type dependencies, pure-body validation, conservative replay cost, explicit top-level rejection |
| `cgx2-notebook-kernel/src/project.rs` | Recursive direct pure-import validation/lowering with branch-local binding sets |
| `cgx2c`, notebook CLI/UI/format/session | Delegate to checked consumers; native/package, HTTP and persistence suites exercise integration; no additional AST walker |

## Verification

Commands use `RUSTUP_TOOLCHAIN=1.98.1`, private target `.target-wave24`, and
`TMPDIR=$PWD/.tmp-wave24`. Opted-in bridge linking uses the pre-existing
`LIBRARY_PATH=/tmp/cgx2-wave22-lib` workaround. Default native builds do not need
that workaround. No other lane's measured binaries were overwritten.

- `cargo test --workspace --all-features --offline --target-dir .target-wave24`
  — passed: 751 tests, 0 failures, 5 ignored, across 184 suite reports.
  Includes opted-in in-process bridge tests and
  all 20 loopback HTTP tests. The ordinary sandbox denies socket binding; the
  complete final run uses granted loopback access.
- `cargo clippy --offline --all-features --all-targets --target-dir .target-wave24`
  with `-p` for all 15 owned crates and `-- -D warnings` — passed. After adding the
  direct-import regression, notebook-kernel Clippy was rerun and passed.
- `cargo +1.98.1 fmt --all -- --check` — passed.
- `git diff --check` — passed.

Focused checks include true/false guards, one evaluation, inactive division by
zero/overflow, taken error code/span/order, nested record returns, shadowing and
mutable outer state, Unit fallthrough, explicit/inferred/missing returns,
dead-branch type/effect/capability checking, memory/proof rejection, Unicode spans,
formatter round trips, native debug/release and Python/package parity, atomic
output preservation, taint return joins, notebook inactive dependencies and
transactional recovery. Existing cancellation, limits, foreign isolation, memory,
package, CLI and persistence regressions also pass in the workspace suite.

Five real-LibTorch tests remain ignored because they require separately reviewed,
pinned external installations/artifacts. Environment-gated real runtime probes
are not evidence of real LibTorch execution here. Trusted live proof and benchmark
work are outside this deliverable.
