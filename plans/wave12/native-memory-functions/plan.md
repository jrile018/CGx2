# Compile checked memory functions natively

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** `cgx2c build` must compile and run the exact Wave 11 checked local memory-function profile, including calls through `main`, with the same runtime session and observable behavior as `cgx2 run`.

**Architecture:** Consume the Wave 11 parser/checker and memory-operation representation as frozen. Lower each accepted zero-argument, Unit-returning local memory function to Rust functions that receive one shared `cgx2_runtime::Session` reference. Reuse the existing portable embedded runtime build and its debug/release profile; no second collector or independent region table. Native output must retain CGx2 source spans in diagnostics and runtime failures. Avoid string matching or unchecked source-to-Rust insertion.

**Ownership:** Only `crates/cgx2-emit-rust/**` and `crates/cgx2c/**`. Read source/checker/runtime code, but do not edit it, root manifests/lock, docs, or plans. Do not use a new API from the parallel object-graph lane.

## Required behavior

1. Freeze valid and invalid Wave 11 memory-function fixtures before changing lowering. Support the already accepted local calls, manual/auto regions, explicit collect, `no_gc`, owned/managed conversions, local drop/use, and scalar result/report behavior actually expressible in that profile. Reject recursion, cross-call memory handles, dynamic sizes, foreign calls, and any unsupported form with a source-located target diagnostic.
2. Compare `cgx2 run` and native debug/release for success and expected failure: manual collection counts, `no_gc` zero collections and budget failure, conversion failure retaining ownership, region limit errors, and program exit code. A failed compile or unsupported program must never install or overwrite the requested output.
3. Preserve portable moved-compiler behavior, bounded stderr, private temporary build paths, release optimization of both program and runtime, and package lock/output protection from Wave 11 integration.
4. Document the native supported subset and explicit unsupported forms in owned READMEs. The generated code may be readable Rust, but the `.cg` program remains authoritative.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-emit-rust -p cgx2c --offline`, `cargo fmt --all --check`, and targeted `cargo clippy --all-targets --offline -- -D warnings`. Include one moved-binary native memory-function smoke. Commit owned paths and report SHA, parity cases, diagnostic examples, runtime profile and limits.

**Acceptance:** An accepted Wave 11 local memory function builds and executes natively in one session; unsupported or unsafe forms fail clearly before producing a binary.
