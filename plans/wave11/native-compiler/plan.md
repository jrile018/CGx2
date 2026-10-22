# Portable native memory compiler

> Work in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Remove the Wave 10 native memory compiler's build-time source-checkout dependency and ensure `--release` optimizes both generated program and runtime.

**Architecture:** Ship a version-matched runtime artifact or embed the exact runtime source needed by the generated memory script, using a reproducible installation layout. `cgx2c` must not resolve `../cgx2-runtime/src/lib.rs` relative to its own compile-time `CARGO_MANIFEST_DIR` at execution. Keep the generated program calling `cgx2-runtime`, with one runtime implementation and the existing memory-report/error semantics. The runtime compile/link profile must match the requested debug or release output.

**Tech stack:** Rust 1.98; `cgx2-emit-rust`, `cgx2c`, `rustc`; offline Cargo.

**Spec:** `docs/language-design.md` CLI/source emission and memory sections; `docs/validation-plan.md` native owned/managed runtime and portability gates.

## Constraints

- Own only `crates/cgx2-emit-rust/**` and `crates/cgx2c/**`. Read but do not edit runtime, source grammar/checker/CLI, package, root manifests/lock, docs/plans. Build only the Wave 10 checked memory-script profile; do not depend on the parallel memory-functions lane's new forms.
- Preserve `cgx2c build` output/diagnostics and the accepted scalar/package paths. Reject source foreign declarations and unsupported memory forms with a source span. Avoid hidden network fetches or an installation requirement for the original repository checkout.
- A moved compiler binary must build and run a memory `.cg` fixture after its original source path is unavailable. Test this using a disposable source copy built under `/tmp`, then remove only that disposable copy; never move or delete the real checkout. Verify release flags apply to both runtime and generated program; compare generated/runtime outputs with `cgx2 run` in both debug and release. Record runtime version/content hash or an equivalent immutable identity in build diagnostics so LLMs and humans can see what was linked. Do not treat optimization as permission to alter collection order or errors.

## Tasks and checks

1. Write a failing test that copies the built `cgx2c` to a temporary installation directory and invokes it on a memory script with its source checkout path hidden/unavailable. Add a test that inspects debug/release runtime compiler arguments or output metadata, plus native-versus-CLI parity for success, `no_gc` error and manual collection.
2. Implement the smallest portable runtime supply mechanism; cache builds only when source/version/target/profile match, if caching is added. Preserve bounded stderr, private build directories, and output replacement safety.
3. Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-emit-rust -p cgx2c --offline`, format, targeted Clippy `-D warnings`, and manual moved-binary debug/release smoke. Update owned READMEs with installation requirements and scope.
4. Commit owned paths and report proof of source-free execution, profiles, checks, limitations and SHA.

**Acceptance:** Moving `cgx2c` away from its checkout does not break native memory scripts, and release timing measures an optimized runtime as well as optimized generated code.
