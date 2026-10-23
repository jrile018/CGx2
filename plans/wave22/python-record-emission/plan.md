# Portable immutable scalar record emission to Python

**Goal.** Carry the committed standalone nominal scalar record subset through checked Python source emission while preserving constructor and field semantics.

**Ownership.** Edit only `crates/cgx2-emit-python/**`, including owned tests and fixtures. Read but do not edit syntax, sema, CLI, package, notebook, root Cargo files, or shared docs. If the checked API cannot safely distinguish a record field from a qualified module name, report the exact boundary rather than changing another lane.

## Deliverable and contract

1. Accept a checked standalone module with immutable `record` declarations of one to eight `i64`/`Bool` fields, named construction, field reads, and local pure calls carrying records. Emit a nominal immutable Python class with declaration-order storage and predictable display. Preserve left-to-right argument evaluation, first error, and checked `i64` arithmetic in field expressions. Reject unsupported records before emitting any partial program.
2. Reuse committed `ExprKind::Name(Path)` and `ExprKind::Call`. `TypedModule` has function signatures but no per-expression binding-resolution map, so reconstruct scoped local record binding types from the already checked AST, initializer types, and parameter signatures, including shadowing, before deciding that `p.x` is a field rather than a qualified module name. If this cannot be done safely within this crate, report the exact blocker. Preserve existing scalar and range-loop Python output and explicit Python-only foreign calls. Do not change grammar or AST variants, and do not claim arbitrary Python consumers enforce CGx2 types.
3. Test reordered named fields, edge `i64`, Bool fields, local function parameter/result, shadowing, unknown/duplicate/missing fields, and emitted Python versus `cgx2 run` output/error class. Explicitly reject mutable, nested, generic, managed, foreign-layout, and cross-package public record use. `cgx2-emit-python/src/package.rs` currently lowers only scalar package values; leave package import/export unsupported in this lane and verify it fails clearly rather than silently miscompiling.

## Verification and handoff

Run offline emitter tests and targeted cross-target record fixtures with an available Python interpreter, `cargo fmt --all --check`, and owned Clippy `-D warnings`. Commit only owned paths; report SHA, example `.cg` and emitted Python, exact results, and remaining portability/type-enforcement limits. Stop for review.
