# CGx2 Baseline Semantic Checker Implementation Plan

> **For agentic workers:** Implement this plan task by task in your attached worktree. Do not delegate, launch another chat, or edit another lane. Track the steps below and stop after your committed deliverable.

**Goal:** Statically check names, basic types, mutability, calls, and returns for the existing Wave 1 `.cg` AST.

**Architecture:** A new `cgx2-sema` library consumes `cgx2_syntax::Module` without changing syntax. It resolves local/function names into a typed summary and returns stable byte-span diagnostics. The checker is intentionally separate from runtime execution until coordinator integration.

**Tech Stack:** Rust 1.98, Cargo offline, `cgx2-syntax` path dependency, standard library.

**Spec:** `docs/language-design.md`, especially syntax/types and compiler pipeline; `plans/wave2/README.md` fixes this wave's subset and ownership.

## Global Constraints

- Own only `crates/cgx2-sema/**` and this plan file. Do not modify syntax, CLI, runtime, root manifests, or shared design docs.
- Keep source spans zero-based, half-open UTF-8 byte offsets; diagnostics expose `code`, `message`, and `span`.
- The supported types are `i64`, `Bool`, `String`, `Unit`, and `List<T>` for parameter/return checking. Reject unknown types and unsupported generic forms explicitly.
- A function's final expression is its return value when there is no explicit `return`; top-level scripts may have a final expression. Arithmetic is `i64` only and runtime overflow remains a dynamic error.
- Reserve preview builtins `arg_count() -> i64` and `arg(i64) -> String` so the CLI's argument behavior has the same signature. Do not silently add more builtins.

## Review Focus

1. An unknown name inside a nested `for` must report the name's span, not a surrounding block span; test in Task 1.
2. Reassignment to immutable `let` and `+=` on non-`i64` must fail; test in Task 2.
3. An integer literal outside signed `i64` must fail without panicking; test in Task 2.
4. A call with wrong arity or argument type must identify the call site; test in Task 3.
5. A public function lacking an explicit return type, or whose tail value disagrees with it, must fail; test in Task 3.

## Files and public contract

- Create `crates/cgx2-sema/Cargo.toml`, `src/lib.rs`, `src/types.rs`, `src/check.rs`, `src/diagnostic.rs`, `tests/check.rs`, and `README.md`.
- Export `Type::{I64, Bool, String, Unit, List(Box<Type>)}`, `TypedModule`, `TypeDiagnostic { code: &'static str, message: String, span: Span }`, and `check_module(module: &cgx2_syntax::Module) -> Result<TypedModule, Vec<TypeDiagnostic>>`.
- `TypedModule` exposes a function-signature index and expression types keyed by source `Span`. It does not own or rewrite the syntax tree. Choose stable public structs for these two fields and document them in `README.md`.
- Codes: `CGT001` unknown/duplicate name, `CGT002` unknown/unsupported type, `CGT003` incompatible types, `CGT004` immutable assignment, `CGT005` call arity, `CGT006` public return annotation, `CGT007` out-of-range integer.

### Task 1: Type table and scoped names

- [ ] Add failing `tests/check.rs` cases for typed parameters, inferred local `let`, shadowing in a nested `for`, duplicate function/parameter, and unknown nested name with exact span.
- [ ] Run `cargo test -p cgx2-sema --offline` and record the initial failure.
- [ ] Implement type resolution and lexical scopes in `types.rs`/`check.rs`. Resolve the five supported types and report `CGT001`/`CGT002` without panics.
- [ ] Run the same tests and verify the name/type cases pass.

### Task 2: Expressions and mutation

- [ ] Add failing cases for `i64` arithmetic, `Bool`/`String` literals, out-of-range literal, assignment to immutable and mutable locals, `+=` on `String`, and `for` over `List<i64>` vs non-list.
- [ ] Run the target test and observe failures for missing behavior.
- [ ] Infer expression types, check mutable binding writes and `for` element types, and populate the `Span -> Type` summary. Never infer success for an unsupported form.
- [ ] Run the target test; verify the Review Focus 2–3 cases pass.

### Task 3: Functions, tail returns, and handoff

- [ ] Add failing cases for private/public functions, explicit and tail returns, builtin `arg`/`arg_count`, unknown function, wrong arity, wrong argument type, and public return annotation.
- [ ] Run the target test to see the intended failures, then implement signature prepass and call/return checking.
- [ ] Run `cargo test -p cgx2-sema --offline`, `cargo fmt --check --manifest-path crates/cgx2-sema/Cargo.toml`, and `cargo clippy -p cgx2-sema --all-targets --offline -- -D warnings` from the repository root.
- [ ] Document the API, exact supported subset, diagnostic codes, and unimplemented effects/capabilities/ownership checks. Commit only owned paths and stop for review.

**Acceptance:** the checker rejects invalid source with exact byte spans and returns a typed summary for valid Wave 1 source. Passing this crate does not imply memory safety or an end-to-end `cgx2 check` command.
