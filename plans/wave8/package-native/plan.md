# CGx2 Native Scalar Package Execution Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Build and run a checked local multi-file `.cg` package whose entry function calls imported public scalar functions, using the Wave 7 package graph and Wave 6 native Rust backend.

**Architecture:** Add `cgx2c build --package ROOT --out BIN [--release] [--force]` and `cgx2c emit-rust --package ROOT --out FILE.rs [--force]`. `cgx2-package` supplies an immutable checked source graph and source hashes after canonical path validation. Extend `cgx2-emit-rust` with a package emitter that lowers each module in dependency order, encodes module/function names into collision-free Rust identifiers, and preserves CGx2 checked scalar errors. Recheck source at build time; never compile an index if source bytes changed after validation. Use the existing safe artifact staging and `rustc` argument-vector process.

**Tech stack:** Rust 1.98, existing package/syntax/sema/Rust emitter/compiler crates, offline rustc.

**Specification:** `crates/cgx2-package/README.md`, `crates/cgx2-emit-rust/README.md`, `crates/cgx2c/README.md`, `plans/wave8/README.md`.

## Owned paths and limits

- Own only `crates/cgx2-package/**`, `crates/cgx2-emit-rust/**`, `crates/cgx2c/**`, and this plan. Do not edit syntax, sema, CLI, root files, memory runtime, or other lanes. Preserve old public APIs/commands and their tested outputs.
- Entry is the manifest `entry` module with a zero-argument `main` function returning `i64`, `Bool`, `String`, or `Unit`. It may call public functions from explicitly imported, listed local modules and private functions in its own module. Imported calls with wrong arity/type, a private target, missing import, cycle, unsupported return type, or stale source must fail before rustc with source-located diagnostics. Script-style package entries and foreign dependencies are outside this slice.
- Supported functions use the existing checked scalar emitter profile: direct calls, local bindings/assignment, returns, strings/Bool/i64/Unit, and checked arithmetic. Memory regions, `no_gc`, Python/PyTorch, dynamic calls, loops, effects/contracts and recursion remain target errors. Do not silently drop import declarations or compile unsupported operations as no-ops.
- Validate module identifier mangling against dotted-name collisions and Rust keywords. Preserve the currently specified overflow/division-by-zero, argument, call-depth, and step-limit behavior in debug and optimized binaries. If exact interpreter parity is unavailable for multi-file programs, use independent expected-output/error oracles and compare the same functions in a single-file equivalent where possible; disclose the difference.
- Maintain safe output behavior: explicit destination, collision requires `--force`, source/output identity rejected, temporary artifacts cleaned on failure, no shell execution. Metadata includes package manifest/source hashes, toolchain, target, and profile. The saved Rust output is deterministic for a fixed checked package.

## Tasks

### Public package build route

- [ ] Add red integration fixtures: three-module successful main, private/missing/wrong-typed calls, cycle, name collision, changed source, unsupported memory source, and output collision.
- [ ] Implement checked package snapshot and emitter; document any new public API. Compile and run one native package binary; compare to independent output oracle.
- [ ] Ensure standalone file build/emit remains byte-for-byte compatible where documented.

### Differential and failure checks

- [ ] Test debug/release behavior, Unicode and shell-metacharacter paths, overflow and division-by-zero, source changes between validation and build, and a failing `rustc` preserving an existing output.
- [ ] Add a complete README example for `cgx2c build --package` and `emit-rust --package`, including exact target exclusions.

### Verification and handoff

- [ ] Run `cargo test -p cgx2-package -p cgx2-emit-rust -p cgx2c --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-package -p cgx2-emit-rust -p cgx2c --all-targets --offline -- -D warnings`.
- [ ] Commit only owned paths; report a runnable package example, checked exclusions, metadata, and any parity limit.

**Acceptance:** A manifest-listed, statically checked scalar package builds to a runnable native executable with behavior-preserving imported calls and safe failure/artifact handling.
