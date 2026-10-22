# CGx2 Python Bridge Core Implementation Plan

> **For agentic workers:** Use the executing-plans skill task by task. This chat owns one bounded deliverable; do not spawn another agent, edit shared files, merge, or start a second assignment. Stop for user review after verification.

**Goal:** Deliver an isolated native Rust-to-CPython bridge with safe handle lifetimes, dynamic calls, clear Python errors, and a typed PyTorch adapter surface.

**Architecture:** A PyO3 host attaches to the Python interpreter linked for this build. Opaque owned Python handles are distinct from CGx2-managed allocations. Generic import/getattr/call operations support arbitrary Python modules; a `TensorHandle` wrapper implements common PyTorch calls through the same boundary without pretending the compiler has checked unwrapped APIs.

**Tech Stack:** Rust stable 1.98.1, PyO3 `=0.29.2` with embedded-interpreter support, Python 3.14.4 currently available. PyO3 is cached locally; `torch` is **not installed** in this Python environment.

**Spec:** `docs/language-design.md` interoperability and foreign-memory sections; `docs/validation-plan.md` PyTorch and foreign-boundary gates.

## Global constraints

- Own only `crates/cgx2-python-bridge/**` and `plans/python-bridge/plan.md`; no edits to root manifests, runtime, compiler, or shared docs.
- This first slice targets the Python interpreter selected when the Rust crate is built. Do not claim arbitrary runtime interpreter switching.
- CGx2 owns PyO3 handles; Python/PyTorch own underlying object and tensor memory. Do not charge it to CGx2 regions or promise `no_gc` stops Python GC.
- No unsafe raw-pointer exchange or zero-copy tensor claim in this slice. Convert small scalar/list data explicitly and document copies.
- Missing PyTorch must produce a clear `ModuleUnavailable` result. A mock is not a substitute for the later real-PyTorch gate.

## Public contract

Create `crates/cgx2-python-bridge/Cargo.toml`, `src/lib.rs`, `src/host.rs`, `src/value.rs`, `src/tensor.rs`, `tests/stdlib.rs`, `tests/torch_optional.rs`, and `README.md`.

- `PythonBridge::new() -> Result<Self, BridgeError>` initializes/attaches once and records Python version/prefix.
- `PyHandle` is an opaque owned Python object handle; cloning/dropping follows PyO3 refcount rules.
- `BridgeValue` covers `None`, `Bool`, `I64`, `F64`, `String`, and `Handle` for this slice.
- `import_module(name)`, `getattr(handle, name)`, `call(handle, args)`, and `to_value(handle)` return `Result<_, BridgeError>`.
- `BridgeError` distinguishes `ModuleUnavailable`, `AttributeMissing`, `TypeConversion`, and `PythonException` with exception type/message and traceback text where available.
- `TensorHandle` wraps a `PyHandle`; `tensor_from_f32(values, shape)`, `shape`, `dtype`, `device`, and `add` are typed entry points. Unsupported dtype/device/shape cases fail explicitly.

## Review focus

1. Missing modules and attributes must not panic or be mislabeled as a compiler error.
2. Repeated calls and dropped handles must not leak references in a bounded refcount test.
3. A Python exception must preserve type, message, and useful traceback.
4. Tensor shape mismatch and dtype conversion must fail clearly; no silent cast or zero-copy claim.
5. The optional real-torch test must report **skipped because torch is absent**, not “passed with a mock.”

## Tasks

### T1: Embedded host and dynamic standard-library calls

- depends_on: []
- owns: [`crates/cgx2-python-bridge/**`]
- non_goals: [CGx2 syntax/compiler, tensor zero-copy]
- consumes: [foreign-boundary spec, cached PyO3]
- produces: [`PythonBridge`, `PyHandle`, `BridgeValue`, errors]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Create failing tests importing `math`, calling `sqrt(9.0)`, reading `sys.version_info`, missing module/attribute, type conversion error, and a Python function that raises.
- [ ] Run `cargo test --manifest-path crates/cgx2-python-bridge/Cargo.toml --offline` and confirm new tests fail before implementation.
- [ ] Implement attach/import/getattr/call/conversion using PyO3 safe APIs; preserve traceback strings.
- [ ] Run the crate tests; document the linked Python version and how `PYO3_PYTHON` selects the build environment.

### T2: Handle lifetime and typed tensor adapter

- depends_on: [T1]
- owns: [`crates/cgx2-python-bridge/**`]
- non_goals: [GPU ownership transfer, DLPack, all of torch API]
- consumes: [dynamic Python host]
- produces: [refcount tests and typed `TensorHandle` API]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Add failing tests for repeated handle clone/drop behavior, tensor module unavailable, invalid shape, and typed operation error mapping.
- [ ] Run tests to observe missing behavior, then implement lazy `torch` import and typed calls. Do not create a fake torch module for the integration test.
- [ ] Keep `tests/torch_optional.rs` conditional on actual `import torch`; when unavailable it prints an explicit skip reason and asserts no success claim. If torch is present in another selected interpreter, run a CPU tensor add/shape/dtype/device round trip.
- [ ] Run the crate tests and inspect the PyO3 dependency resolution; report any offline build limitation.

### T3: Boundary documentation and review receipt

- depends_on: [T2]
- owns: [`crates/cgx2-python-bridge/**`, `plans/python-bridge/plan.md`]
- non_goals: [installing PyTorch without user review, claiming GPU benchmarks]
- consumes: [working host and tensor wrapper]
- produces: [documented safe bridge core]
- estimate: S
- coordination_risk: low
- status: pending

- [ ] Run `cargo fmt --manifest-path crates/cgx2-python-bridge/Cargo.toml --check`, `cargo clippy --manifest-path crates/cgx2-python-bridge/Cargo.toml --offline -- -D warnings`, and crate tests.
- [ ] Document interpreter selection, reference ownership, copied value types, exception mapping, missing-torch result, and the later real-torch acceptance gate.
- [ ] Commit owned paths and stop. Report test results, interpreter/torch availability, commit ID, and any bridge API concerns.

**Acceptance:** standard-library calls work through the real linked CPython; owned handles and exceptions behave correctly; typed tensor entry points compile and fail clearly when torch is missing. Real PyTorch interoperability remains unverified until torch is installed.
