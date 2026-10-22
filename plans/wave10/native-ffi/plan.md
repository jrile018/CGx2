# Direct native library boundary and LibTorch prototype

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Establish a testable generic native C ABI call boundary and a direct LibTorch adapter prototype. No Python interpreter, PyO3 call, or Python fallback may be used for the LibTorch route.

**Architecture:** New `cgx2-ffi-native` crate validates a versioned declaration and invokes a tiny compiled C fixture through declared symbols, fixed-width scalars and owned opaque handles. New `cgx2-libtorch` crate uses the published native LibTorch interface where available or a narrow version-pinned C++ facade compiled against LibTorch. The LibTorch adapter is conditional on an installed compatible distribution; inability to run it here is an explicit open gate.

**Tech stack:** Rust 1.98, C/C++ toolchain for fixtures, native dynamic loading or link binding as appropriate, LibTorch only if locally installed; offline Cargo. Do not require CPython for either new crate.

**Spec:** `docs/interoperability-design.md` foreign contract v1 and acceptance gates; `docs/interoperability-top10.md`; `docs/validation-plan.md` interoperability targets.

## Scope and contract

- Own only new `crates/cgx2-ffi-native/**` and `crates/cgx2-libtorch/**`. Do not edit root `Cargo.toml`, `Cargo.lock`, shared docs/plans, CLI/source crates, or existing Python bridge. `crates/*` automatically registers the new crates. Keep dependencies available offline; if a loader crate is unavailable, use a narrow platform-specific checked loader or build-time static fixture, clearly documenting its support boundary.
- Declaration v1 must identify library/ABI/version, symbol, fixed-width parameter/result types, ownership/release operation, error channel, and supported OS/architecture. Reject missing symbol, unsupported calling convention/type, incompatible declared version, and duplicate release before an unsafe call where detectable. Raw pointers remain inside an `unsafe` adapter. Foreign exceptions never unwind through Rust/CGx2 frames.
- C fixture tests cover successful tiny scalar call, UTF-8 or byte buffer copy, owned opaque handle release, intentionally wrong symbol/version, wrong return/error, and resource cleanup on failure. Report copies/allocations; do not claim zero-copy unless tested. Avoid an unsound generic bit-cast ABI dispatcher.
- LibTorch prototype should expose a small typed CPU tensor operation (create, add, shape, release) through native code and return typed errors. Preserve dtype/device and owner rules. If no compatible LibTorch is installed, compile or validate the adapter as far as possible without it and report the real CPU/GPU conformance tests as unavailable. Do not simulate a passing PyTorch test with a fake tensor.
- This wave creates the host boundary only. `.cg` syntax, `cgx2 bind check`, ten-language adapters, GPU claims, and package management remain subsequent integration work.

## Tasks

1. Inspect available offline libraries and toolchains; record exact supported platforms/versions in owned READMEs. Add failing C fixture tests for declaration validation, call/error translation, and handle release. Implement the smallest safe native layer.
2. Implement the direct LibTorch adapter as a separate crate/facade with no Python dependency. Add a conditionally run real LibTorch CPU test, and a static dependency check that the route does not link/load `libpython`. Keep GPU tests conditional on an available device.
3. Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-ffi-native -p cgx2-libtorch --offline`, fmt, targeted Clippy `-D warnings`, and the C fixture. Report separately which real LibTorch tests executed versus were skipped.
4. Commit only owned paths and report the declaration schema, supported route, proof of no Python dependency, exact tests, remaining source/binding-package gap, and commit.

**Acceptance:** A real C library fixture is called through a checked native declaration, and the LibTorch path is structurally direct and honest about unavailable runtime validation.
