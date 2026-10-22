# CGx2 Checked CLI and Structured Diagnostics Implementation Plan

> **For agentic workers:** Implement this plan task by task in your attached worktree. Do not delegate or edit another lane. Commit the bounded deliverable and stop for user review.

**Goal:** Make `cgx2 check` perform real Wave 2 semantic checking and make command-line `run` reject statically invalid source before execution.

**Architecture:** Keep `cgx2_cli::run_source` as the existing low-level preview interpreter API for backward compatibility. Add a parse-and-check layer used by the command-line `check` and `run` paths. Centralize diagnostic rendering so human text and JSON Lines describe the same source span and code.

**Tech Stack:** Rust 1.98, existing `cgx2-cli`, `cgx2-syntax`, and `cgx2-sema`; standard library for JSON encoding.

**Spec:** `docs/language-design.md` CLI/LLM interface and `plans/wave3/README.md` frozen API contract.

## Global Constraints

- Own only `crates/cgx2-cli/**` and this plan file. Do not change parser, semantic checker, runtime, root manifests, or shared docs.
- Preserve every existing CLI and interpreter test. `run_source` remains available as a low-level evaluator; CLI `run` now checks source first.
- `cgx2 check FILE.cg` exits 0 only after both parse and semantic checks succeed. It does not claim ownership/effect/capability correctness yet; say so in the help and README.
- Add `--diagnostic-format=json` to `check` and `run`. Emit one JSON object per diagnostic with `code`, `severity`, `message`, `file`, and `span` `{start,end}`. Spans remain zero-based half-open UTF-8 bytes. Escape control characters and quotes correctly; no debug formatting as JSON.
- Preserve `cgx2 run FILE.cg -- ARGS` and shortcut forms. Static failure must not execute the program or print a success value. Runtime failure remains a separate `CGR` diagnostic.
- The static checker may accept a `for` statement that the preview interpreter cannot execute. `check` succeeds if semantics pass; `run` returns explicit `CGR009` unsupported construct. Do not claim these commands have identical supported subsets.

## Review Focus

1. A syntactically valid but ill-typed program must fail `check` and CLI `run` before output; test in Task 2.
2. Multiple type errors must yield multiple JSON Lines, each parseable and with an exact UTF-8 byte span; test in Task 3.
3. A filename or diagnostic message containing quotes/backslashes must not break JSON; test in Task 3.
4. A parser error retains its `CGS` code, type error its `CGT` code, and runtime error its `CGR` code; test in Task 3.
5. Existing `arg`/`arg_count` programs must remain valid and forward spaced Unicode arguments; test in Task 2.

## Files and public contract

- Modify `crates/cgx2-cli/Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/error.rs`; create `src/check.rs`, `src/diagnostics.rs`, `tests/check.rs`; update `README.md` and existing tests if needed.
- Export `check_source(source: &str) -> Result<cgx2_sema::TypedModule, Vec<CheckDiagnostic>>` and `CheckDiagnostic { code: String, message: String, span: Span }`. Parsing errors become diagnostics with their existing `CGS` codes; semantic errors keep `CGT` codes.
- A CLI JSON diagnostic is one line per error, with a package-relative or given input path string. Human text may show line/column as additional context but cannot replace the stable byte span in JSON.

### Task 1: Parse-and-check library layer

- [ ] Add failing tests for valid scalar script/module, invalid name/type/immutable assignment, and parse error with original codes/spans.
- [ ] Run `cargo test -p cgx2-cli --offline` and record initial failures.
- [ ] Implement `check_source` from public parser and semantic checker APIs without duplicating type logic.
- [ ] Re-run the tests and verify valid source returns `TypedModule` and invalid source returns stable diagnostics.

### Task 2: Checked command paths

- [ ] Add failing process tests for `check` exit status, `run` refusing static errors, no success stdout on failure, valid `arg` forwarding, and a semantically valid `for` that run reports unsupported.
- [ ] Run the target tests to observe failures, then integrate `check_source` into `main.rs` command dispatch. Preserve the low-level `run_source` API and existing tests.
- [ ] Re-run process tests and CLI smoke `cargo run -p cgx2-cli --offline -- check crates/cgx2-cli/tests/fixtures/answer.cg`.

### Task 3: Diagnostic formats and handoff

- [ ] Add failing tests for multiple JSON Lines, valid escaped JSON strings, UTF-8 byte spans, and CGS/CGT/CGR code provenance.
- [ ] Run them red, then implement one diagnostic renderer used by check/run; verify JSON with a parser in the test or a small, exact decoder assertion.
- [ ] Run `cargo test -p cgx2-cli --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --check --manifest-path crates/cgx2-cli/Cargo.toml`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-cli --all-targets --offline -- -D warnings`.
- [ ] Document what `check` validates and does not validate. Commit only owned paths, report commit and checks, then stop.

**Acceptance:** CLI `check` uses the semantic checker, CLI `run` statically gates invalid programs, JSON Lines diagnostics are valid and byte-span precise, and all Wave 2 CLI behavior remains tested.
