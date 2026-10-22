# CGx2 Checked Scalar Rust Emitter Plan

> Implement this plan in your attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Emit readable Rust source for the already checked scalar CGx2 subset and prove behavior matches the CGx2 CLI on fixed fixtures.

**Architecture:** Add `cgx2-emit-rust` as a library with `emit_rust(module: &cgx2_syntax::Module, checked: &cgx2_sema::TypedModule) -> Result<String, Vec<EmitDiagnostic>>` (or an equally explicit API). Reject stale/mismatched summaries and unsupported AST constructs. Generate a self-contained Rust `main` and helper functions for checked `i64` arithmetic, string escaping, function calls, tail returns, `arg`/`arg_count`, and source-like readable names. The emitter targets a pure scalar profile; it must reject memory regions, owned values, `no_gc`, Python calls, effects, and unsupported loops.

**Tech stack:** Rust 1.98, standard library and existing syntax/sema crates, `rustc` in differential tests. No network dependencies.

**Specification:** `docs/language-design.md` portability section, `crates/cgx2-emit-python/README.md`, `plans/wave5/README.md`.

## Owned paths and fixed contracts

- Own only new `crates/cgx2-emit-rust/**` and this plan. Root wildcard workspace discovers the crate; do not edit syntax, sema, CLI, Python emitter, runtime, root manifests/lock, or docs.
- Require a successful `cgx2_sema::check_module` result. Validate that the supplied summary belongs to the supplied module, including public/private signatures and expression span types, or recheck internally if that is safer. A manually fabricated summary must not let unchecked source emit.
- Match CGx2 `i64` checked arithmetic. In Rust debug and release, `+`, `-`, `*`, `/` overflow must produce deterministic CGx2-compatible errors rather than panic/wrap. Division by zero and `i64::MIN / -1` need explicit handling. Arguments and Boolean/string display must match CLI output.
- Names that collide with Rust keywords/helper identifiers must be escaped or renamed deterministically. Preserve CGx2 source meaning across Unicode and string escapes. A diagnostic has stable code, message, and UTF-8 byte span.
- Do not claim this is the full native backend or that memory scripts compile natively. Generated source is editable; roundtrip edits into `.cg` are not required.

## Review focus

1. Fixed scripts/functions yield identical stdout and exit status through `cgx2 run` and compiled Rust, including arguments, checked arithmetic boundaries, and Unicode strings.
2. Checked failures produce a nonzero exit with a defined message, never a Rust panic trace or overflow wrap.
3. Memory, `no_gc`, named arguments, loops, and qualified/unknown calls return an explicit unsupported-target diagnostic at the relevant span.
4. Emission is deterministic, line-oriented, and human-readable; helper names cannot collide with user names.
5. Stale semantic summaries and malformed ASTs cannot bypass validation.

## Tasks

### 1. Pure scalar mapping

- [x] Add red tests for scalar script, `let`/mutation, function module, tail return, `arg`/`arg_count`, identifier escaping, Unicode/string literals, and deterministic output.
- [x] Implement validation and readable Rust emission. State precise supported profile in the crate README.
- [x] Ensure generated source can compile with installed `rustc` without Cargo or external crates.

### 2. Differential and rejection tests

- [x] Add fixture tests that write generated Rust to a temporary directory, invoke `rustc` with `--edition=2024`, run the binary, and compare stdout/exit/error class with `cgx2-cli` for valid/error cases. Avoid relying on exact compiler formatting.
- [x] Test `i64::MIN`, max/min overflow, divide-by-zero, recursion depth where supported, and malformed strings. If the CGx2 runtime limits cannot be preserved, reject that construct rather than diverge silently.
- [x] Add rejection tests for memory, loops, foreign calls, and stale summaries with spans.

### 3. Verification and handoff

- [x] Run `cargo test -p cgx2-emit-rust --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-emit-rust --all-targets --offline -- -D warnings`.
- [x] Document how to call the emitter and compile output, plus full-backend gaps. Commit only owned paths.

**Acceptance:** Real generated Rust binaries behave like the checked CGx2 scalar preview on deterministic fixtures; unsupported semantics are diagnosed rather than mistranslated.
