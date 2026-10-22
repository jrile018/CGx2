# CGx2 Python bridge core

This standalone crate embeds the CPython interpreter selected by PyO3 when the crate is **built**. `PythonBridge::new()` attaches to that interpreter and records `sys.version` and `sys.prefix`. Set `PYO3_PYTHON` before building to select a Python executable and its environment; changing it at runtime does not switch interpreters. Embedded builds need the matching Python development library for linking, as well as the Python runtime.

```rust
use cgx2_python_bridge::{BridgeValue, PythonBridge};

fn main() -> Result<(), cgx2_python_bridge::BridgeError> {
    let bridge = PythonBridge::new()?;
    let math = bridge.import_module("math")?;
    let sqrt = bridge.getattr(&math, "sqrt")?;
    let result = bridge.call(&sqrt, &[BridgeValue::F64(9.0)])?;
    assert!(matches!(bridge.to_value(&result)?, BridgeValue::F64(3.0)));
    Ok(())
}
```

`import_module`, `getattr`, `call`, and `to_value` attach to Python for each operation. `PyHandle` owns a PyO3 reference. Cloning it increments Python's reference count, and dropping it releases that reference according to PyO3's rules. Clones alias the same Python object; this bridge does not promise exclusive access. `BridgeValue` copies `None`, Boolean, signed 64-bit integer, 64-bit float, and UTF-8 string values. All other objects remain handles, except a Python integer outside `i64`, which returns `TypeConversion`. Calls through the dynamic path are checked at runtime by Python, not by the CGx2 compiler.

`BridgeError` distinguishes missing modules, missing attributes, conversion failures, and Python exceptions. Error records retain the Python exception class, message, and formatted traceback when Python supplies one. A missing `torch` import is `ModuleUnavailable`; it is not a successful tensor operation.

`tensor_from_f32(values, shape)` checks that dimensions and the element count fit Python's signed size, and that the count matches `values.len()` before allocating. It copies values into a new CPU `torch.float32` tensor and returns `TensorHandle`. `tensor_from_handle` validates a dynamically obtained tensor. `shape`, `dtype`, and `device` inspect it; `dtype` accepts only `torch.float32`, and `device` accepts only `cpu`. `add` and `mul` require equal shapes, dtype, and device; neither broadcasts. `matmul` requires rank-2 shapes `(m, k)` and `(k, n)`, and checks that the `(m, n)` element count is representable before calling Python. New typed results are checked for shape, dtype, and device. Validation failures use `BridgeError::TypeConversion` with the expected condition and actual shape/value; Python operation failures use `PythonException` with class, message, and traceback. `TensorHandle` clones alias the same Python tensor. GPU tensors, other dtypes, implicit broadcasting, ownership transfer, DLPack, and zero-copy exchange are outside this slice. Python and PyTorch own object and tensor storage; CGx2 owns only the handle. Their allocations are foreign and are not charged to CGx2 regions. A CGx2 `no_gc` block cannot stop Python GC or PyTorch allocation.

`PythonBridge::probe_torch()` returns `TorchProbe { python_version, python_prefix, status }`. `TorchStatus::Available { torch_version }` means the selected interpreter imported `torch`, constructed a real CPU float32 `torch.Tensor`, and returned the expected scalar from both typed multiplication and matmul. `Unavailable` means the top-level `torch` package is absent; `Broken` retains an import, version, or capability error, plus the version when it could be read. A successful import or a module named `torch` alone is not enough for `Available`. The probe describes the interpreter linked into this build, not a Python executable selected later at runtime.

Run `cargo test --manifest-path crates/cgx2-python-bridge/Cargo.toml --offline -- --nocapture`. To select a Python environment with real PyTorch, set `PYO3_PYTHON=/path/to/python` **before building** and rerun the command; that environment must also provide its matching development library. On a system that has only a versioned runtime library such as `libpython3.14.so.1.0`, the link step needs the matching unversioned development-library link name (`libpython3.14.so`). A temporary symlink can supply it through `LIBRARY_PATH` without changing system files. The optional tests import the **real** `torch` package. If it is absent, they print `SKIPPED real PyTorch` notices and assert the unavailable path. With PyTorch installed they check creation, addition, multiplication, rectangular matmul, zero-sized output, result values, shape/dtype/device, and rejected integer tensors. A skipped test is a missing external acceptance gate, not evidence that real PyTorch works. This suite does not establish GPU behavior or the latency targets in `docs/validation-plan.md`. Connecting this bridge to `.cg` imports remains later integration work.
