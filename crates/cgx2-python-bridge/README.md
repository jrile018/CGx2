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

`tensor_from_f32(values, shape)` checks the shape before allocating, copies values into a new CPU `torch.float32` tensor, and returns `TensorHandle`. `tensor_from_handle` validates a dynamically obtained tensor. `shape`, `dtype`, and `device` inspect it; `dtype` accepts only `torch.float32`, and `device` accepts only `cpu`. `add` requires equal shapes, dtype, and device and returns another tensor handle. `TensorHandle` clones alias the same Python tensor. GPU tensors, other dtypes, implicit broadcasting, ownership transfer, DLPack, and zero-copy exchange are outside this slice. Python and PyTorch own object and tensor storage; CGx2 owns only the handle. Their allocations are foreign and are not charged to CGx2 regions. A CGx2 `no_gc` block cannot stop Python GC or PyTorch allocation.

Run `cargo test --manifest-path crates/cgx2-python-bridge/Cargo.toml --offline`. On a system that has only a versioned runtime library such as `libpython3.14.so.1.0`, the link step also needs the matching unversioned development-library link name (`libpython3.14.so`). The test environment for this slice supplied that name through a temporary `LIBRARY_PATH` symlink; no system library was changed. `tests/torch_optional.rs` imports the **real** `torch` package. If it is absent, the test prints `SKIPPED real PyTorch integration` and checks the missing-module result. The real-torch acceptance gate remains pending until a selected interpreter has PyTorch installed; then the test exercises CPU tensor creation, shape/dtype/device inspection, addition, and a rejected integer tensor. This suite does not establish GPU behavior or the latency targets in `docs/validation-plan.md`.
