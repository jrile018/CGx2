# CGx2 First Executable Preview Implementation Plan

> **For agentic workers:** Implement this plan task by task in your attached worktree. Do not delegate, launch another chat, or edit another lane. Track the steps below and stop after your committed deliverable.

**Goal:** Run a small, honest subset of `.cg` programs from the command line with stable errors and forwarded program arguments.

**Architecture:** A new `cgx2-cli` binary/library parses the frozen Wave 1 AST, interprets pure scalar scripts/functions, and prints the final non-`Unit` value. It makes no static type-safety or native-code performance claim. A later coordinator pass will connect the semantic checker and memory runtime.

**Tech Stack:** Rust 1.98, Cargo offline, `cgx2-syntax` path dependency, standard library.

**Spec:** `docs/language-design.md` CLI section and `plans/wave2/README.md` preview seam.

## Global Constraints

- Own only `crates/cgx2-cli/**` and this plan file. Do not modify syntax, semantic checker, runtime, root manifests, or shared docs.
- Support `cgx2 run FILE.cg -- ARGS`, `cgx2 FILE.cg -- ARGS`, and `cgx2 fmt FILE.cg` in this preview. Do not advertise `check`, `build`, `test`, or proof commands yet.
- Execute top-level scripts in order, or call a zero-argument `main` in a function module. Report missing/ambiguous `main` or unsupported constructs instead of silently skipping them.
- Value types are `i64`, `Bool`, `String`, and `Unit`. Integer operations use checked signed `i64`; division by zero and `i64::MIN / -1` are explicit errors. A final expression's non-`Unit` value is written to stdout with one newline.
- Reserve only `arg_count() -> i64` and `arg(i64) -> String` as preview builtins. `arg(0)` is the first argument after `--`; a missing or negative index is a runtime error. The semantic checker plan uses the same signatures.
- Cap executed statements/expressions at 1,000,000 steps by default to stop accidental infinite recursion; state the limit in an error. No arbitrary shell command execution is provided by source code in this preview.

## Review Focus

1. A nonexistent or unreadable `.cg` file must exit nonzero with a clear path error; test in Task 3.
2. `i64` overflow and division by zero must produce nonzero exit without wrapped output; test in Task 2.
3. Immutable assignment and unknown function must produce distinct runtime errors; test in Task 2.
4. `--` must separate CGx2 options from program arguments, preserving spaces and Unicode; test in Task 3.
5. Recursive `main` must hit the step cap rather than hang or overflow the host stack; test in Task 2 (add a depth cap if needed).

## Files and public contract

- Create `crates/cgx2-cli/Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/eval.rs`, `src/error.rs`, `tests/run.rs`, `tests/cli.rs`, `tests/fixtures/*.cg`, and `README.md`.
- Binary target name is `cgx2`. Export `Value::{I64(i64), Bool(bool), String(String), Unit}`, `RunResult { value: Value }`, `RunError { code: &'static str, message: String, span: Option<Span> }`, and `run_source(source: &str, args: &[String]) -> Result<RunResult, RunError>`.
- `main.rs` owns file I/O, argument parsing, stdout/stderr, and exit status. `eval.rs` owns interpreter state. Syntax diagnostics retain their `CGS` code; runtime codes start `CGR`.

### Task 1: Scalar evaluator

- [ ] Add failing `tests/run.rs` cases for literal/tail values, precedence, `let`, mutable `+=`, function call/return, and script vs function-module entry.
- [ ] Run `cargo test -p cgx2-cli --offline` to record failures.
- [ ] Implement source parsing and scoped scalar evaluation. Use the Wave 1 AST without changing it; report unsupported `For`/other forms explicitly.
- [ ] Run the test again and verify the core cases pass.

### Task 2: Errors and bounded execution

- [ ] Add failing cases for overflow, divide by zero, bad operand type, immutable assignment, missing name/function, missing `main`, out-of-range `arg`, and recursion/step cap.
- [ ] Run the target test to observe failures, then implement checked arithmetic, stable `RunError` codes/spans, step and depth limits.
- [ ] Re-run the target test; failures must leave no success output.

### Task 3: CLI shell contract

- [ ] Add process-level `tests/cli.rs` cases using temporary `.cg` fixtures: both run forms, `fmt`, forwarded Unicode/spaced arguments, file error, runtime error, and exit code.
- [ ] Run the test to see missing CLI behavior, then implement `main.rs` argument parsing and human-readable diagnostics. `fmt` prints canonical text; it does not rewrite the file.
- [ ] Run `cargo test -p cgx2-cli --offline`, `cargo fmt --check --manifest-path crates/cgx2-cli/Cargo.toml`, and `cargo clippy -p cgx2-cli --all-targets --offline -- -D warnings` from the repository root.
- [ ] Document exact preview syntax/commands, unsupported features, and distinction from the planned compiler. Commit only owned paths and stop for review.

**Acceptance:** `cargo run -p cgx2-cli --offline -- run path/to/example.cg -- hello` runs a valid preview program; tests prove nonzero exits on bad inputs. This is an interpreter preview, not proof of native performance, static memory safety, or full CGx2 semantics.
