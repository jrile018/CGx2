# CGx2 Native Scalar Compiler Command Plan

> Implement this plan in your attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Compile a checked scalar `.cg` file into a native executable by using the Wave 5 Rust emitter and installed `rustc`, with clear target limits and safe file handling.

**Architecture:** New `cgx2c` binary crate reads one `.cg`, parses and checks it, invokes `cgx2_emit_rust::emit_rust`, writes generated Rust into a private temporary build directory, and invokes `rustc` through an argument vector with `--edition=2024` and an explicit output path. It atomically installs the completed binary only after compile succeeds. Provide `cgx2c emit-rust FILE.cg --out FILE.rs` to inspect readable generated source. No shell command construction, package imports, foreign libraries, or memory script compilation.

**Tech stack:** Rust 1.98, existing syntax/sema/Rust-emitter crates, standard library, installed `rustc`, offline.

**Specification:** `docs/language-design.md` CLI and target semantics, `crates/cgx2-emit-rust/README.md`, `plans/wave6/README.md`.

## Owned paths and fixed boundaries

- Own only new `crates/cgx2c/**` and this plan. Do not edit main `cgx2` CLI, Rust emitter, parser, sema, root manifests/lock, or docs.
- Commands: `cgx2c build FILE.cg --out BINARY [--release]` and `cgx2c emit-rust FILE.cg --out FILE.rs`. Require explicit output path. `--release` passes an optimization level to `rustc`; default is a correct debug build. No silent overwrite; require explicit `--force` when output exists.
- Parse/check before invoking `rustc`. If Rust emitter rejects memory/loops/foreign calls/recursion, surface its code and source span. A `rustc` failure is distinct from CGx2 source diagnostics, includes bounded stderr, and leaves existing output intact. `rustc` path is selected from a controlled CLI flag or environment with no shell expansion and recorded in build metadata.
- Temporary generated source and partially built binary must not be left as final output on error. If the requested output equals the source file, reject it even with `--force`. Avoid following symlinks for dangerous overwrite targets where feasible; document platform limits.
- Do not claim full native CGx2 or that all first-release memory semantics compile. The emitted scalar binary must match interpreter output/errors for the supported fixtures.

## Review focus

1. `cgx2c build` creates a runnable native binary from a scalar fixture; output and exit status match `cgx2 run` on arguments and checked arithmetic boundaries.
2. `emit-rust` creates deterministic readable Rust which compiles with the documented command. Repeating same source/toolchain gives identical generated text.
3. Syntax/type/target/rustc/I/O errors have distinct codes and do not overwrite an existing artifact or create a misleading success binary.
4. Paths with spaces, Unicode, and shell metacharacters are passed as literal arguments; no shell execution is used.
5. Build metadata records toolchain/target/source hash or explicitly documents what provenance is unavailable; no performance claim follows from a working build.

## Tasks

### 1. Build/emit command

- [ ] Add red integration tests for command parsing, scalar build, emit-rust, output collision, missing source, and unsupported memory source.
- [ ] Implement the parse/check/emit/rustc pipeline with bounded temp files and atomic final installation. Use `Command` argument vectors only.
- [ ] Add a small `.cg` example and README invocation.

### 2. Differential and failure behavior

- [ ] Compare built binary with `cgx2` scalar CLI for at least six fixtures including arguments, Unicode, integer overflow, and division by zero. Test debug and release where practical.
- [ ] Use a fake failing `rustc` executable to verify compiler failure keeps the previous output untouched and reports stderr safely.
- [ ] Test paths with spaces and an attempted output equal to input.

### 3. Verification and handoff

- [ ] Run `cargo test -p cgx2c --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2c --all-targets --offline -- -D warnings`.
- [ ] Commit only owned paths and report a native executable example. State that memory/foreign/packages are unsupported by this build route.

**Acceptance:** A user can build and run a native binary for the checked scalar profile with safe artifacts and differential evidence; full native CGx2 remains a later milestone.
