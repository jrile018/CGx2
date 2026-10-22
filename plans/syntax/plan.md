# CGx2 Syntax Candidate Implementation Plan

> **For agentic workers:** Use the executing-plans skill task by task. This chat owns one bounded deliverable; do not spawn another agent, edit shared files, merge, or start a second assignment. Stop for user review after verification.

**Goal:** Deliver a standalone parser and canonical formatter for the provisional indentation-based CGx2 `.cg` syntax, with source-located errors.

**Architecture:** A UTF-8 lexer emits tokens with half-open byte spans and INDENT/DEDENT/NEWLINE markers. A small recursive-descent parser builds an AST for modules, functions, and the expression/statement subset needed by current examples. Formatting prints one canonical four-space style and reparses to the same AST.

**Tech Stack:** Rust stable 1.98.1, standard library only.

**Spec:** `docs/language-design.md` (source language and diagnostics), `docs/validation-plan.md` (syntax comparison).

## Global constraints

- Product name CGx2, source extension `.cg`; syntax is a **candidate**, not the final frozen grammar.
- Own only `crates/cgx2-syntax/**` and `plans/syntax/plan.md`. Do not edit root manifests, shared specs, benchmark data, or another crate.
- No semantic type checking or Rust code generation. Expose a clean AST and errors so the later semantic compiler can consume them.
- Spans are zero-based, half-open UTF-8 byte offsets. The formatter uses four spaces, newline-terminated statements, and no routine semicolons.
- Fail with diagnostics, not panics, for malformed source.

## Public contract

Create `crates/cgx2-syntax/Cargo.toml`, `src/lib.rs`, `src/lexer.rs`, `src/ast.rs`, `src/parser.rs`, `src/format.rs`, `tests/syntax.rs`, and `README.md`.

- `Span { start: usize, end: usize }` uses byte offsets.
- `SyntaxDiagnostic { code: &'static str, message: String, span: Span }`.
- `parse_module(source: &str) -> Result<Module, Vec<SyntaxDiagnostic>>`.
- `format_module(module: &Module) -> String`.
- `Module` has optional dotted module name, function items, and source span. `Function` has public flag, name, typed parameters, return type, body, and span. The AST represents `let`/`let mut`, assignment, return/tail expression, calls, names, literals, binary arithmetic, and `for` blocks. Types are preserved as syntax, not resolved.
- Diagnostic codes start with `CGS` and remain stable within this first slice; document them in the crate README.

## Review focus

1. A tab used for indentation must get a precise `CGS` diagnostic at its byte offset.
2. Blank lines and comments inside a block must not change indentation depth.
3. CRLF and non-ASCII identifiers must preserve correct byte spans.
4. A missing colon or unexpected dedent must be reported without a panic.
5. Formatting twice must be idempotent and keep parse-equivalent structure.

## Tasks

### T1: Standalone crate and lexer

- depends_on: []
- owns: [`crates/cgx2-syntax/**`]
- non_goals: [type resolution, brace grammar, code generation]
- consumes: [`docs/language-design.md`]
- produces: [token stream and `Span` contract]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Create the crate and tests `lex_simple_function`, `lex_nested_indentation`, `reject_tab_indentation`, `retain_unicode_byte_spans`, and `ignore_comment_only_indentation`.
- [ ] Run `cargo test --manifest-path crates/cgx2-syntax/Cargo.toml --offline` and observe the new tests fail before lexer behavior exists.
- [ ] Implement tokens for names, keywords, punctuation, literals, operators, NEWLINE, INDENT, DEDENT, and EOF. Reject tabs in leading indentation; recover from bad tokens.
- [ ] Run the same command; all lexer tests pass, including CRLF and UTF-8 spans.

### T2: AST and parser

- depends_on: [T1]
- owns: [`crates/cgx2-syntax/**`]
- non_goals: [type checking, borrowed-value semantics]
- consumes: [token stream]
- produces: [`parse_module` and documented AST]
- estimate: L
- coordination_risk: low
- status: pending

- [ ] Add failing parse tests for the two functions in the specification, a top-level script, nested `for`, call/arithmetic precedence, missing colon, and unexpected dedent.
- [ ] Run the crate test command and confirm parse cases fail for missing behavior.
- [ ] Implement recursive-descent parsing with bounded error recovery at NEWLINE/DEDENT, preserving spans on every node.
- [ ] Run the crate test command; valid cases return AST, invalid cases return stable `CGS` diagnostics and never panic.

### T3: Canonical formatter and handoff

- depends_on: [T2]
- owns: [`crates/cgx2-syntax/**`, `plans/syntax/plan.md`]
- non_goals: [choosing the final grammar from model benchmarks]
- consumes: [AST]
- produces: [`format_module`, fixture corpus, public API README]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Add failing formatter tests for four-space indentation, blank-line placement, idempotence, and parse/format/parse AST equivalence on every valid fixture.
- [ ] Run the crate test command to observe formatter failures, then implement a deterministic printer with a trailing newline.
- [ ] Run `cargo fmt --manifest-path crates/cgx2-syntax/Cargo.toml --check`, `cargo clippy --manifest-path crates/cgx2-syntax/Cargo.toml --offline -- -D warnings`, and the crate tests.
- [ ] Document syntax supported and rejected, diagnostic codes, AST API, and the remaining grammar decisions in the crate README.
- [ ] Commit only owned paths. Stop and report test results, commit ID, limitations, and any proposed interface change for the later compiler.

**Acceptance:** the exact checks above pass offline; both specification examples parse; malformed indentation and syntax get byte spans; formatting is idempotent. No claim is made that indentation has beaten braces for LLMs.
