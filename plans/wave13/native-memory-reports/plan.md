# Observable native memory parity

> Use the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Make native memory-script and Wave 12 local-function binaries expose an opt-in machine-readable memory report so the coordinator can compare actual collection counts, limits and failure state with `cgx2 run`.

**Architecture:** Emit one versioned report path from the same `cgx2_runtime::Session` that executes generated code. Choose a bounded opt-in invocation flag or explicitly documented environment switch that leaves ordinary stdout/stderr and program arguments unchanged when absent. Report on both success and expected failure after the runtime state is settled; do not label a report fresh if the program never started. Reuse runtime report fields rather than rebuilding counters in the emitter. Keep generated Rust deterministic and source-located runtime errors intact.

**Ownership:** Only `crates/cgx2-emit-rust/**` and `crates/cgx2c/**`. Read runtime/CLI code but do not edit it, root manifests/lock, docs or plans. Target frozen Wave 12 checked memory scripts and zero-argument Unit local memory functions. Do not consume the parallel source-reference lane's new syntax or the assurance lane's new hook.

## Required behavior

1. Tests must compare native debug and release **actual report values** with interpreter values for explicit manual collection, `no_gc` zero collection, nested guard budget failure, region limit failure, owned/managed conversion failure, per-region live/peak/failure counts and exit status. The Wave 12 test only checked interpreter collection counts; replace that gap with native evidence. Compare reports by semantic fields, not raw path/time noise.
2. Unsupported source, compiler failure or invalid output path must produce no report and preserve an existing binary/output. Report output cannot be mistaken for program stdout or a success value after error. Cap report size, keep a stable schema/diagnostic code, and state whether wall-clock pause fields may differ.
3. Preserve moved-compiler portability, embedded runtime identity, debug/release optimization, package lock protection and existing scalar/native behavior. Do not claim parity for source object graphs or unsafe foreign C calls.
4. Document exact flag/default, schema version, field units, error behavior and parity limits in owned READMEs.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-emit-rust -p cgx2c --offline`, format and targeted Clippy `-D warnings`; include a moved-binary report smoke. Commit owned paths and report SHA plus a concrete native/interpreter report comparison.

**Acceptance:** Native success and failure fixtures expose matching semantic collection/failure counts, including zero collections inside `no_gc`.
