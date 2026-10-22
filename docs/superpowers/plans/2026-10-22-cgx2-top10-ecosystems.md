# CGx2 Ten-Ecosystem Library Access Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make one representative package in each of ten selected language ecosystems callable and checked from `.cg`, using shared adapter engines and explicit runtime requirements.

**Architecture:** Build on the versioned declaration, compiler checks, and native FFI in `2026-10-22-cgx2-interoperability.md`. Each ecosystem adapter resolves a locked package artifact and implements the same typed call/error/ownership contract. Native routes call directly; runtime and worker routes are visible in the package manifest and diagnostics.

**Tech Stack:** Rust CGx2 workspace; C ABI; existing PyO3 crate; Node.js, JVM/JNI, .NET `hostfxr`, PHP CLI, Go `c-shared` or process; ecosystem package managers and lockfiles.

**Spec:** `docs/interoperability-top10.md`, `docs/interoperability-design.md`. Start after the common interoperability foundation and Wave 9 review, so active worker paths are not overwritten.

## Global Constraints

- These are ten tested **routes**, not ten universal import mechanisms. A package is supported only after its binding and platform matrix pass acceptance.
- Every `.cg` call is typed, declares effects/capabilities, and preserves CGx2 memory rules. Foreign allocations and collections are not controlled by CGx2 `no_gc`.
- PyTorch always uses the native LibTorch package adapter, never the generic Python route.
- Dependency installation is explicit. Record exact resolved artifacts and transitive locks. No import-time installation or implicit route fallback.
- Keep each adapter behind a small stable interface; an ecosystem's process, VM, or compiler details may not leak into normal `.cg` code.

## Review Focus

- A transitive package update changes a lock: reject rather than silently use it (Task 1).
- A Node Promise, Java exception, .NET exception, Python exception, or PHP throwable becomes a typed foreign error without crashing CGx2 (Tasks 3–7).
- A C/C++/Rust/Go callback or destructor cannot outlive its recorded owner (Tasks 8–11).
- A failed package probe or unsupported architecture gives a diagnostic before the first user call (all tasks).
- A `torch` import cannot resolve to PyPI merely because a package with that name is installed (Task 3).

---

### Task 1: Shared package lock and adapter interface

**Files:** Create `crates/cgx2-bindings/{Cargo.toml,src/lib.rs,src/lock.rs,src/adapter.rs,tests/lock.rs}` and `crates/cgx2-interop-worker/{Cargo.toml,src/,tests/}`; modify `crates/cgx2-package/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**` and `cgx2.toml` documentation as needed. Consume the prior plan's `ForeignDeclarationV1`.

**Interfaces:** Define `ForeignValueV1` (bounded scalar/string/bytes/opaque handle), `ForeignCallV1` (binding key, arguments, declared effects), `ForeignResponseV1` (`Result<ForeignValueV1, ForeignError>`), and an adapter `probe/invoke` interface. `cgx2 bind add ECOSYSTEM:PACKAGE@VERSION` must resolve explicitly; `cgx2 bind check PACKAGE` must verify the locked artifact and runtime without user-call side effects where possible.

- [ ] Add failing tests for deterministic lock generation, missing artifact, version/hash drift, unlisted transitive dependency, unsupported platform, and a route mismatch. Test worker framing for malformed length and oversized messages.
- [ ] Implement one adapter registration and manifest/lock validation path plus the bounded versioned worker transport; run targeted binding, package, CLI, API-index, and worker tests. Commit after review.

### Task 2: JavaScript/npm via Node worker

**Files:** Create `crates/cgx2-interop-node/{Cargo.toml,src/,worker/,tests/}` and `.cg` acceptance fixture. Use the shared worker transport from Task 1.

- [ ] Test a locked `lodash.camelCase` call, missing export, rejected effect, Promise result, thrown error, timeout, crash, and oversized response from `.cg`.
- [ ] Implement a persistent pinned Node worker with bounded versioned messages, no shell evaluation, exact export allowlist, and typed error/async mapping. Run offline fixture and real-package acceptance. Commit.

### Task 3: Python/PyPI and direct PyTorch separation

**Files:** Extend the reviewed `crates/cgx2-python-bridge/**` and `crates/cgx2-foreign/**`; add `.cg` source adapter and acceptance fixtures only after their Wave 9 changes land.

- [ ] Test a locked Python-only `python-dateutil` function from `.cg`, a Python exception, wrong environment, leaked handle, and denied capability. Verify `torch` resolves to LibTorch and the native test process loads no CPython.
- [ ] Connect the explicit `python` route to source checking and the existing bridge while retaining build-time interpreter constraints until a reviewed alternative is implemented. Run bridge, source, and LibTorch regression tests. Commit.

### Task 4: TypeScript/npm through the Node engine

**Files:** Extend `crates/cgx2-interop-node/**` and declaration generation code in `crates/cgx2-bindings/**`.

- [ ] Test a pinned JavaScript build of `date-fns` from `.cg`; compare declared runtime exports with `.d.ts` candidates and reject a declaration whose export exists only in types.
- [ ] Reuse Task 2's Node worker; add type-declaration import as a suggestion tool with explicit effect/ownership review. Run paired JavaScript/TypeScript package tests. Commit.

### Task 5: Java/Maven through JNI

**Files:** Create `crates/cgx2-interop-jvm/{Cargo.toml,src/,tests/}` and `.cg` acceptance fixture.

- [ ] Test a locked Apache Commons Lang `StringUtils.capitalize` call, wrong classpath, exception, cross-thread invocation, and object-reference cleanup.
- [ ] Host a version-pinned JVM through the Invocation API; implement thread attachment and scoped/global reference rules. Run fixture and real-package tests. Commit.

### Task 6: C#/NuGet through .NET hosting

**Files:** Create `crates/cgx2-interop-dotnet/{Cargo.toml,src/,managed/,tests/}` and `.cg` acceptance fixture.

- [ ] Test Newtonsoft.Json serialization, wrong assembly/runtime configuration, managed exception, GC handle lifetime, and an incompatible second runtime request.
- [ ] Host a compatible framework-dependent .NET runtime through `hostfxr` and invoke a reviewed managed facade. Keep runtime configuration in the lock. Run fixture and real-package tests. Commit.

### Task 7: PHP/Composer through the shared worker protocol

**Files:** Create `crates/cgx2-interop-php/{Cargo.toml,worker/,tests/}`; use the shared worker transport from Task 1.

- [ ] Test a locked `ramsey/uuid` text result, Composer autoload failure, PHP throwable, worker crash/timeout, malformed frame, and message-size limit.
- [ ] Implement the version-pinned PHP process on the shared framed transport. Run Node regression plus PHP fixture and real-package acceptance. Commit.

### Task 8: C library route

**Files:** Add `.cg` zlib fixture and version/platform checks to `crates/cgx2-ffi-native/**` from the foundation plan.

- [ ] Test zlib compression/decompression, undersized output, missing symbol, declared ownership, and a denied effect from `.cg`.
- [ ] Add the reviewed binding and validate against a direct C baseline. Commit.

### Task 9: C++ library facade route

**Files:** Create `crates/cgx2-interop-cpp/tests/` with a small `fmt` C ABI facade and `.cg` fixture; use `cgx2-ffi-native` for loading.

- [ ] Test `fmt::format` through the facade, C++ exception translation, destructor behavior, and deliberate version/toolchain mismatch.
- [ ] Build the pinned facade and run `.cg` acceptance and native comparison. Commit.

### Task 10: Rust crate facade route

**Files:** Create `crates/cgx2-interop-rust/tests/` with a Rust `extern "C"` facade around `regex` and `.cg` fixture.

- [ ] Test compile, match/non-match, invalid regex error, owner destruction, and panic containment at the foreign boundary.
- [ ] Build the pinned crate facade and run `.cg` acceptance and direct Rust comparison. Commit.

### Task 11: Go module facade and worker fallback

**Files:** Create `crates/cgx2-interop-go/{Cargo.toml,facade/,worker/,tests/}` and `.cg` fixture.

- [ ] Test one `golang.org/x/text` operation, wrong Go build/version, pointer-lifetime rejection, and an unsupported `c-shared` platform using the worker fallback only when explicitly declared.
- [ ] Build the reviewed `c-shared` facade on supported platforms; route all other declared configurations through the Task 7 worker. Run fixture and direct Go comparison. Commit.

## Whole-project gate

Run the common foundation suite and all ten `.cg` package fixtures on supported CI platforms. Produce `routes passing/10`, `tested package count`, runtime/platform matrix, memory-failure report, cold and warm call latency, bulk throughput, copies, and peak memory versus direct host-language callers. Run held-out LLM package-use tasks using the generated API index and compare verified completion, time, and tokens. An engine or host API without a running `.cg` fixture does not count as a passing route.
