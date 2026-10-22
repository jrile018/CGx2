# Native memory and contract execution

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Extend `cgx2c`/Rust source emission so the existing bounded `.cg` memory-script profile can build and run natively, and scalar functions with Wave 9 `requires`/`ensures` cannot silently lose their contracts in generated Rust.

**Architecture:** Lower the already checked memory-script AST to readable Rust that uses public `cgx2-runtime::Session` operations, preserving allocation, collection, borrow, move, promote, copy, `no_gc`, and errors. Use the existing runtime API; do not duplicate its collector. For scalar functions, emit pre/postchecks or reject unsupported contract forms with a source span before producing output. Keep package-scalar emission behavior stable.

**Tech stack:** Rust 1.98, `cgx2-emit-rust`, `cgx2c`, existing `cgx2-syntax`, `cgx2-sema`, `cgx2-runtime`; offline Cargo and native `rustc`.

**Spec:** `docs/language-design.md` memory and source-emission sections; `docs/validation-plan.md` memory, CLI and portability gates.

## Scope and contract

- Own only `crates/cgx2-emit-rust/**` and `crates/cgx2c/**`. Read, but do not edit, parser, checker, CLI memory interpreter, runtime, root manifest/lockfile, docs or plans. Use Wave 9 AST/contracts from the shared base; do not depend on the source-Python lane's new syntax. The runtime lane preserves current public `Session` signatures.
- Begin with the currently accepted literal-size memory script profile (`crates/cgx2-cli/tests/fixtures/memory.cg`) and its operations. Preserve manual collection only on explicit command and `no_gc` zero-collection behavior. Print/report only outputs that the CLI profile already defines. Native failures must exit nonzero and identify the source operation or a clear generated-runtime error.
- Do not present source code that only calls the CLI evaluator as native memory emission. Generated Rust must call `cgx2-runtime` directly. A tiny generated runtime helper is acceptable; a second collector is not.
- In both single-module and package scalar emitters, preconditions fail before body execution and postconditions after successful return, or emission stops with a specific diagnostic and source span. Effects/capabilities metadata may be erased only when the checker has proved the emitted path respects them; unsupported combinations must be rejected. No false claim of SMT proof.
- Keep unsupported memory-in-function, mixed scalar/memory, foreign, nonliteral size, and nested constructs explicit. Do not expand source grammar or type rules in this wave.

## Tasks

1. Add failing differential tests: CLI `cgx2 run` versus compiled binary for success, manual/auto region behavior, `no_gc` budget failure, invalid handle/borrow, and owned/managed conversion. Include at least one script from the existing fixture and an error path. Compile generated Rust in the test with the local runtime dependency; avoid downloading dependencies.
2. Implement the bounded memory lowering in emitter and `cgx2c` dispatch using current static memory checks. Preserve source-located diagnostics where possible. Add an explicit rejection fixture for an unsupported form.
3. Add native contract differential tests for true/false `requires` and `ensures`, output/error ordering, and a pure function that previously would have silently dropped metadata. Implement checks or diagnostic rejection in both emitters.
4. Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-emit-rust -p cgx2c --offline`, fmt, targeted Clippy `-D warnings`, and one native memory CLI build/run command. Document the supported slice in owned READMEs. Commit only owned paths and report the commit, checks, exact supported forms and gaps.

**Acceptance:** A checked memory script reaches `cgx2-runtime` in a native executable, and native scalar emission never silently discards an active contract.
