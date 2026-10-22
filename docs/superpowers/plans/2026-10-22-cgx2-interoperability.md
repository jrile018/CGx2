# CGx2 Interoperability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `.cg` programs call supported foreign libraries through explicit, checked contracts, with direct native LibTorch access to PyTorch and separate adapters for runtime-only libraries.

**Architecture:** A versioned declaration describes the target's ABI, values, ownership, and effects. The compiler checks a typed `.cg` facade against it and lowers calls through a route-specific adapter. Raw bit and pointer manipulation stays in audited `unsafe foreign` adapter code.

**Tech Stack:** Existing Rust 1.98 CGx2 workspace, native C ABI, optional LibTorch C/C++ stable ABI, existing PyO3 bridge only for explicit Python-only routes. WebAssembly components and other managed-runtime adapters are expansion tracks.

**Spec:** `docs/interoperability-design.md`. Read `docs/language-design.md` and the completed Wave 9 integration review; reconcile their Python-hosted PyTorch language before changing code.

**Companion roadmap:** `docs/interoperability-top10.md` defines the first ten language ecosystems, reusable adapter engines, representative packages, and per-ecosystem release gates. This plan supplies their common native foundation; the managed-runtime and worker adapters follow as separately reviewed work.

## Global Constraints

- Do not modify an active Wave 9 worker's owned paths; start implementation after those changes are reviewed and integrated or assigned new isolated ownership.
- `.cg` PyTorch calls use LibTorch without loading Python. Existing Python bridging remains a separate route for Python-only packages.
- Safe `.cg` code cannot construct or dereference arbitrary foreign pointers. Foreign allocators and garbage collectors remain outside CGx2 `no_gc` budgets.
- Every new public call carries declared effects, capabilities, ownership, error behavior, and source-facing diagnostics.
- Unsupported libraries, platforms, operations, and ABI versions produce explicit errors; no implicit fallback to Python or RPC.

## Review Focus

- A release callback invoked twice must not double-free a foreign handle; Task 3 tests this.
- A wrong ABI version or symbol must fail at load time before the call; Task 2 tests this.
- A borrowed CGx2 buffer must not escape a foreign call unless an explicit owner-retention contract exists; Task 3 tests this.
- A C++ exception must become `ForeignError`, never unwind into CGx2 frames; Task 4 tests this.
- A `torch` import must not load CPython on the native path; Task 4 tests loaded dependencies and behavior.

---

### Task 1: Reconcile the specification and define the declaration schema

**Files:** Modify `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`; create `crates/cgx2-ffi-core/{Cargo.toml,src/lib.rs,src/declaration.rs,tests/declaration.rs}`. Update root `Cargo.toml` only if workspace membership requires it.

**Interfaces:** Produce `ForeignDeclarationV1`, `ForeignTypeV1`, `ForeignRouteV1`, and `parse_declaration(source: &str) -> Result<ForeignDeclarationV1, DeclarationError>` as the sole schema entry point. Include exact platform, ABI, symbol, version, type, ownership, destructor, error, thread, and effect fields from the spec. Follow workspace schema conventions discovered during implementation; reject unknown fields.

- [ ] Write schema fixtures for a valid C `add(i32,i32)->i32`, valid opaque owned handle, and rejection of missing release function, unannotated pointer, unknown route, duplicate symbol, unknown field, and oversized declaration. Confirm the new tests fail.
- [ ] Implement the parser and validator, then run `cargo test -p cgx2-ffi-core --offline`; expect all schema tests to pass.
- [ ] Update the three design documents so PyTorch is specified as direct LibTorch and Python remains optional for Python-only packages. State precisely which routes belong to first usable release.
- [ ] Review the schema against `docs/interoperability-design.md`, commit only this task's paths, and record any deliberately unsupported v1 types.

### Task 2: Compile-time contracts and native scalar calls

**Files:** Create `crates/cgx2-ffi-native/{Cargo.toml,src/lib.rs,src/load.rs,tests/native_scalar.rs}` and a small C fixture under `crates/cgx2-ffi-native/tests/fixtures/`; modify `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, and `crates/cgx2-api-index/**` as required by the exact existing interfaces.

**Interfaces:** Consume `ForeignDeclarationV1`. Add a typed `.cg` foreign import facade that checks parameter/result types and effects before lowering. Add `cgx2 bind check PACKAGE`, which validates library identity, version/ABI, and required symbols before invoking one. Expose bound symbols and effects through the API index.

- [ ] Add a `.cg` fixture that calls the C `add` library and checks its result; add negative fixtures for wrong arity, missing `foreign` effect, unavailable capability, missing symbol, and wrong ABI/version. Confirm expected failures before implementation.
- [ ] Implement cached library loading and scalar calls using the declared platform C calling convention; map load/call failures to stable diagnostic categories. Run targeted sema, CLI, API-index, and FFI tests; expect the positive fixture to run and negatives to fail before any foreign side effect.
- [ ] Test Linux and the supported CI platforms; mark unsupported platforms explicitly rather than silently assuming their ABI. Commit after review.

### Task 3: Ownership, buffers, callbacks, and unsafe boundary

**Files:** Extend `crates/cgx2-ffi-core/**` and `crates/cgx2-ffi-native/**`; add lifetime/alias fixtures under the native crate's `tests/` and source fixtures under existing CGx2 test locations.

**Interfaces:** Add checked opaque handle ownership (`borrowed`, `owned`, `shared` when the target offers retain/release), pointer-plus-length UTF-8 and bytes, and explicit copy or bounded borrowed buffer calls. Add callbacks only after declaring thread, reentrancy, and lifetime rules. Keep raw pointer dereference behind `unsafe foreign`.

- [ ] Add tests for destructor exactly once, clone/retain semantics, error-path cleanup, borrowed buffer escape rejection, wrong string encoding, callback after owner drop, wrong-thread callback, and foreign allocation inside `no_gc` being reported as foreign rather than CGx2 memory. Confirm the unsafe cases are rejected or require `unsafe foreign`.
- [ ] Add bounds and endian tests for safe bit-oriented byte operations; verify an undeclared struct reinterpretation is rejected in safe code.
- [ ] Implement one ownership rule at a time, rerun targeted tests, and use a memory sanitizer or equivalent dynamic check on native fixtures. Do not add a zero-copy path until its owner and alias rules pass tests.
- [ ] Document what the compiler proves, what the runtime checks, and what remains the adapter author's responsibility. Commit after review.

### Task 4: Direct LibTorch package

**Files:** Create `crates/cgx2-libtorch/{Cargo.toml,README.md,src/,tests/}` and `.cg` acceptance fixtures. Modify compiler import/lowering paths only after Tasks 1–3 provide their interfaces.

**Interfaces:** Export a typed CGx2 `Tensor` facade for create, inspect, add, matmul, and one autograd operation supported by the selected LibTorch release. Use PyTorch's limited stable native ABI where it covers an operation; otherwise build a small version-pinned C facade over its C++ API. Handle dtype, device, shape, ownership, aliasing, mutation, and errors explicitly.

- [ ] Add native LibTorch reference tests and matching `.cg` tests for CPU output, lifetime, failure, and optional GPU behavior. Add a dependency check asserting that the CGx2-to-LibTorch process does not load `libpython` or import `torch` through CPython.
- [ ] Implement the adapter and fail at build/load time on unsupported LibTorch versions or missing operations. Run CPU acceptance; record GPU as passed only on a real supported device, otherwise as unavailable.
- [ ] Benchmark cached small calls and bulk tensor operations against equivalent hand-written LibTorch, including copies and peak memory. Report measured results with uncertainty; do not infer a speed win from avoiding Python. Commit after review.

### Task 5: Explicit runtime and data-exchange routes

**Files:** Extend the existing `crates/cgx2-python-bridge/**` and completed `crates/cgx2-foreign/**` only after Wave 9 integration; add separate adapter crates for process/RPC and WebAssembly components if those targets are selected for the next release.

**Interfaces:** Reuse the Task 1 declaration and Task 2 compiler checks. Python-only calls must name the `python` route and its pinned environment; process calls must name a versioned request/response protocol. DLPack and Arrow C Data exchange are separate optional typed adapters, not function-call ABIs.

- [ ] Add fixtures showing one Python-only function works through the explicit Python route, while a PyTorch `.cg` call cannot silently resolve to that route.
- [ ] Add a process fixture with a version mismatch, timeout, crash, and malformed response; require typed errors and bounded payloads. Add WebAssembly, JVM, and .NET adapters as separate plans after their exact runtime contracts and supported platforms are chosen.
- [ ] Compare route availability and overhead with the native route, update API documentation and tooling, and commit reviewed changes.

## Release gate

Complete the spec's acceptance gates with reproducible output: compile/load rejection tests, memory safety evidence, direct LibTorch CPU evidence, optional real GPU evidence, Python-only explicit-route evidence, native baseline benchmarks, and held-out LLM integration tasks. Record unsupported platforms and libraries in a capability matrix. “Compatible with any library” may be claimed only as an adapter architecture; individual packages are supported only when their declared route and tests pass.
