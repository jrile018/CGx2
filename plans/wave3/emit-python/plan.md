# CGx2 Checked Python Source Emitter Implementation Plan

> **For agentic workers:** Implement this plan task by task in your attached worktree. Do not delegate or edit another lane. Commit the bounded deliverable and stop for user review.

**Goal:** Emit readable, executable Python for a deliberately small scalar CGx2 subset while preserving checked signed-`i64` behavior.

**Architecture:** A new `cgx2-emit-python` library consumes the frozen syntax `Module` and successful `TypedModule`. It emits Python functions/scripts plus a compact compatibility prelude for checked integer operations and program arguments. Any unsupported syntax/type returns a target diagnostic with a CGx2 source span; the emitter never silently drops a construct.

**Tech Stack:** Rust 1.98, `cgx2-syntax` and `cgx2-sema` path dependencies, Python 3.14 stdlib for differential tests, no new external crates.

**Spec:** `docs/language-design.md` portability/source-emission section and `plans/wave3/README.md` frozen inputs.

## Global Constraints

- Own only `crates/cgx2-emit-python/**` and this plan file. Do not edit syntax, sema, CLI, root manifests, or shared docs.
- Profile includes scalar literals, `let`/assignment, arithmetic, function calls, explicit/tail returns, scripts or a zero-argument `main`, and the preview `arg_count`/`arg` builtins. Reject `for`, `List<T>`, qualified names, regions, ownership, foreign calls, effects, and any unsupported form with a stable target error.
- Signed `i64` input and arithmetic must raise a clear Python error on out-of-range, division by zero, and `i64::MIN / -1`, matching the CLI's success/failure class and successful value. Python's unbounded integers must not leak through.
- A script's final non-`Unit` value, or a module `main` result, prints with one newline when run as a file. Program arguments correspond to values after the Python script path. Avoid relying on Python's default truthy conversion for CGx2 `Bool`/`i64`.
- Generated identifiers must not become Python keywords, collide with generated helpers, or allow source injection; preserve readable names when safe and mangle deterministically otherwise. Escaped string content must round-trip.
- This is a portable subset emitter, not Python library interoperability. It must not claim that it can emit manual regions or `no_gc` correctly.

## Review Focus

1. `i64::MAX + 1` and `i64::MIN / -1` must fail in emitted Python just as the CLI does; test in Task 2.
2. A CGx2 identifier that is a Python keyword or helper name must not corrupt output; test in Task 2.
3. Strings with newline, quote, backslash, and non-ASCII characters must preserve value; test in Task 2.
4. Unsupported `for`/`List`/qualified call must return an error with exact CGx2 span; test in Task 1.
5. A valid script and `main` function must have identical stdout and exit success to `cgx2` on paired fixtures; test in Task 3.

## Files and public contract

- Create `crates/cgx2-emit-python/Cargo.toml`, `src/lib.rs`, `src/emit.rs`, `src/names.rs`, `src/error.rs`, `tests/emit.rs`, `tests/differential.rs`, fixture `.cg` files, and `README.md`.
- Export `EmitDiagnostic { code: &'static str, message: String, span: Span }` and `emit_python(module: &cgx2_syntax::Module, typed: &cgx2_sema::TypedModule) -> Result<String, Vec<EmitDiagnostic>>`.
- Use `CGE001` for unsupported syntax/type, `CGE002` for inconsistent/stale typed summary, and `CGE003` for unrepresentable identifiers/literals. Do not add a CLI command; the coordinator owns that later integration.

### Task 1: Profile validation and scalar emission

- [ ] Add failing tests for a function and script with literals, arithmetic, `let`, assignments, calls, and unsupported `for`/`List`/qualified forms with spans.
- [ ] Run `cargo test -p cgx2-emit-python --offline` and record failures.
- [ ] Implement profile validation against both AST and typed summary, then deterministic Python emission. Unsupported nodes fail rather than printing an approximation.
- [ ] Re-run tests; emitted files must pass `python3 -m py_compile` in a temporary directory.

### Task 2: Numeric, argument, string, and name semantics

- [ ] Add failing execution tests for checked arithmetic/overflow/division, `arg`/`arg_count`, escaped Unicode strings, Python keyword/helper collisions, and an out-of-range literal rejected upstream.
- [ ] Run target tests red, implement compatibility helpers and name/escape handling, then rerun. Keep helpers small and readable in emitted output.
- [ ] Verify `emit_python` rejects a stale `TypedModule` supplied from a different AST rather than using wrong type information.

### Task 3: Differential fixtures and handoff

- [ ] Add paired fixtures run by `cgx2-cli` and generated Python for at least a script, function module, arguments, and checked error; compare successful stdout and exit success/failure class.
- [ ] Run `cargo test -p cgx2-emit-python --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --check --manifest-path crates/cgx2-emit-python/Cargo.toml`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-emit-python --all-targets --offline -- -D warnings`.
- [ ] Document exact profile, generated helper behavior, unsupported target errors, and why PyTorch is still a bridge target. Commit only owned paths and stop.

**Acceptance:** generated Python runs for the declared pure scalar subset and differential fixtures agree with the CLI on observable successful output and checked-error class. No broader source-portability claim is made.
