# CGx2 Typed PyTorch Bridge Expansion Plan

> Implement this plan in your attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Expand the existing typed CPU `torch.float32` bridge with useful inference math and a trustworthy environment probe, while preserving the dynamic Python escape hatch.

**Architecture:** Add typed `TensorHandle` operations for elementwise multiplication and two-dimensional matrix multiplication. Validate shape/dtype/device before calling real PyTorch; prohibit implicit broadcasting in these typed methods. Add a `PythonBridge::probe_torch()` result that reports interpreter identity, `torch` version when available, supported CPU float32 capability, and a distinct unavailable/error state. Underlying tensors remain owned by Python/PyTorch, while CGx2 owns reference handles.

**Tech stack:** Rust 1.98, existing PyO3 dependency and selected CPython interpreter. Offline build; no package installation or system changes.

**Specification:** `docs/language-design.md` Python interop and memory boundary sections, `crates/cgx2-python-bridge/README.md`, `plans/wave4/README.md`.

## Owned paths and boundaries

- Own only `crates/cgx2-python-bridge/**` and this plan. Do not edit other crates, benchmark, root manifests/lock, or shared docs.
- Preserve `PythonBridge::new`, dynamic `import_module/getattr/call/to_value`, `tensor_from_f32`, `TensorHandle::add`, `shape/dtype/device`, and existing error distinctions. New typed operations are additive.
- Typed `mul` requires identical shapes, CPU device, and float32 dtype; `matmul` requires rank-2 shapes `(m,k)` and `(k,n)` and returns `(m,n)`. Check dimension conversion and overflow before allocation. Avoid making an unchecked zero-copy or GPU claim.
- Real `torch` is optional in this environment. When absent, the probe must report `Unavailable` and real integration tests must say they were skipped. A fake Python module may test error translation but cannot satisfy the real PyTorch gate.
- The project does not control Python GC. Document that CGx2 `no_gc` covers only CGx2 heap collection and foreign allocations remain foreign.

## Review focus

1. Multiplication and matmul return correct CPU float32 results with real PyTorch when installed, including rectangular matrices and zero-sized dimensions if PyTorch supports them.
2. Mismatched shape, rank, dtype, device, and overflow fail before a PyTorch operation with explicit `BridgeError` variants; no implicit broadcasting.
3. Probe never says available merely because a module named `torch` imports; check required tensor operations on a tiny actual tensor.
4. Python exceptions preserve class, message, traceback; cloned handles continue to alias the same object and do not transfer ownership.
5. Missing `torch` is reported as missing external validation, not a successful capability test.

## Tasks

### 1. Typed operations

- [ ] Add red tests for multiplication/matmul shape validation and results. Include malformed dimensions that must fail without invoking PyTorch.
- [ ] Implement `TensorHandle::mul(&self, other: &Self)` and `matmul(&self, other: &Self)` (or clearly documented names) using existing attach/error helpers. Refuse implicit broadcasting.
- [ ] Test that `shape`, `dtype`, and `device` on returned handles match the typed contract.

### 2. Environment probe and real tests

- [ ] Add a public structured probe result that differentiates `Available`, `Unavailable`, and `Broken`/exception; include interpreter path or prefix, Python version, and actual `torch.__version__` when present.
- [ ] Add an optional real-torch test that exercises tiny CPU tensors and both operations. Print a clear skip notice when `torch` is absent. Check the unavailable path as an assertion.
- [ ] Document interpreter selection via `PYO3_PYTHON` at build time and how to run the real acceptance test in a Python environment with `torch`.

### 3. Verification and handoff

- [ ] Run `cargo test --manifest-path crates/cgx2-python-bridge/Cargo.toml --offline`; if linking fails due a missing unversioned `libpython`, use a temporary local `LIBRARY_PATH` link as documented, without changing system files. Record the exact gate result and whether real `torch` was present.
- [ ] Run `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check` and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-python-bridge --all-targets --offline -- -D warnings` if linker/config allows. Update README with precise ownership and capability boundaries. Commit only owned paths.

**Acceptance:** Typed CPU math and capability reporting work where real PyTorch is installed; absent PyTorch is disclosed. Connecting this bridge to `.cg` imports remains a later integration task.
