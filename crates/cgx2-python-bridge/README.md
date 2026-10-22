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

`import_module`, `getattr`, `call`, `call_kwargs`, and `to_value` attach to Python for each operation. `PyHandle` owns a PyO3 reference. Cloning it increments Python's reference count, and dropping it releases that reference according to PyO3's rules. Clones alias the same Python object; this bridge does not promise exclusive access. `BridgeValue` copies supported scalar, byte, and structured data. Other objects remain handles, except a Python integer outside `i64`, which returns `TypeConversion`. Calls through the dynamic path are checked at runtime by Python, not by the CGx2 compiler.

## Dynamic value and keyword API

```rust
pub fn import_module(&self, name: &str) -> Result<PyHandle, BridgeError>;
pub fn getattr(&self, handle: &PyHandle, name: &str) -> Result<PyHandle, BridgeError>;
pub fn call(&self, callable: &PyHandle, args: &[BridgeValue]) -> Result<PyHandle, BridgeError>;
pub fn call_kwargs(
    &self,
    callable: &PyHandle,
    positional: &[BridgeValue],
    keyword: &[(&str, BridgeValue)],
) -> Result<PyHandle, BridgeError>;
pub fn to_value(&self, handle: &PyHandle) -> Result<BridgeValue, BridgeError>;
```

`call` delegates to `call_kwargs` with no keywords. Keyword names must be Python identifiers (including Unicode identifiers), must be unique, and are inserted in lexical order regardless of input order. Duplicate or invalid names and limits produce `BridgeError::TypeConversion` before the callable runs. Python call failures produce `BridgeError::PythonException` with the class, message, and formatted traceback. The bridge does not evaluate source strings or install modules.

| Value | CGx2-owned Rust value | Python object and ownership |
| --- | --- | --- |
| `None`, `Bool`, `I64`, `F64`, `String` | Scalar or copied UTF-8 string | Converted to a Python argument; result converted to a Rust scalar/string copy. Python `int` outside `i64` is a `TypeConversion` error, including inside containers. |
| `Bytes(Vec<u8>)` | Copied bytes | A new Python `bytes`; result bytes copied back. No shared buffer. |
| `List(Vec<BridgeValue>)`, `Tuple(Vec<BridgeValue>)` | Recursively copied container | New Python list/tuple on input; built-in list/tuple copied back on output. A nested `Handle` still aliases its Python object. |
| `Dict(BTreeMap<String, BridgeValue>)` | String-keyed, recursively copied dictionary | New Python dict on input; a built-in string-keyed dict copied back on output. Rust `BTreeMap` and keyword insertion order are lexical. A Python dict with a non-string key stays a handle. |
| `Handle(PyHandle)` | Reference only | Opaque Python object, including unsupported types and subclasses. Clones alias the object; drop releases a reference. When a list/tuple/dict cycle is encountered, the repeated object becomes a nested handle. |

Each `call` or `call_kwargs` validates its combined positional and keyword values before converting them: maximum container depth `MAX_VALUE_DEPTH = 16` (top-level value is depth 0), `MAX_VALUE_ELEMENTS = 4096` total value nodes, `MAX_VALUE_BYTES = 1_048_576` cumulative UTF-8 string/key and byte payload, and `MAX_KEYWORDS = 128`. Each `to_value` uses a fresh depth/element/byte budget and checks it before copying values. Exceeding a limit returns `TypeConversion`; unsupported objects and cycles retain opaque handles. These limits bound bridge traversal and copied payload, not the size of an opaque Python object. Python/PyTorch allocation and garbage collection remain foreign: CGx2 regions and `no_gc` do not control them. Copying a container does not transfer ownership of nested handles or stop their Python side effects.

For example, stdlib `json.dumps` receives a copied dictionary and two keywords:

```rust
use std::collections::BTreeMap;
use cgx2_python_bridge::{BridgeValue, PythonBridge};

let bridge = PythonBridge::new()?;
let json = bridge.import_module("json")?;
let dumps = bridge.getattr(&json, "dumps")?;
let mut object = BTreeMap::new();
object.insert("greeting".into(), BridgeValue::String("雪".into()));
let result = bridge.call_kwargs(
    &dumps,
    &[BridgeValue::Dict(object)],
    &[("ensure_ascii", BridgeValue::Bool(false)), ("sort_keys", BridgeValue::Bool(true))],
)?;
assert!(matches!(bridge.to_value(&result)?, BridgeValue::String(_)));
```

Stdlib `collections.Counter` accepts a keyword and returns an object that remains a handle:

```rust
use cgx2_python_bridge::{BridgeValue, PythonBridge};

let bridge = PythonBridge::new()?;
let collections = bridge.import_module("collections")?;
let counter = bridge.getattr(&collections, "Counter")?;
let counts = bridge.call_kwargs(&counter, &[], &[("apples", BridgeValue::I64(2))])?;
let most_common = bridge.getattr(&counts, "most_common")?;
let pairs = bridge.call(&most_common, &[])?;
assert!(matches!(bridge.to_value(&pairs)?, BridgeValue::List(_)));
```

`BridgeError` distinguishes missing modules, missing attributes, conversion failures, and Python exceptions. Error records retain the Python exception class, message, and formatted traceback when Python supplies one. A missing `torch` import is `ModuleUnavailable`; it is not a successful tensor operation.

Exception records cap the retained class at 256 UTF-8 bytes, message at 4,096 bytes, and traceback at 12,032 bytes (`MAX_PYTHON_FAILURE_BYTES = 16_384` in total). Truncation preserves UTF-8 boundaries. The cap applies after Python formats the exception; it bounds the returned record, not Python's temporary formatting allocation.

`tensor_from_f32(values, shape)` checks that dimensions and the element count fit Python's signed size, and that the count matches `values.len()` before allocating. It copies values into a new CPU `torch.float32` tensor and returns `TensorHandle`. `tensor_from_handle` validates a dynamically obtained tensor. `shape`, `dtype`, and `device` inspect it; `dtype` accepts only `torch.float32`, and `device` accepts only `cpu`. `add` and `mul` require equal shapes, dtype, and device; neither broadcasts. `matmul` requires rank-2 shapes `(m, k)` and `(k, n)`, and checks that the `(m, n)` element count is representable before calling Python. New typed results are checked for shape, dtype, and device. Validation failures use `BridgeError::TypeConversion` with the expected condition and actual shape/value; Python operation failures use `PythonException` with class, message, and traceback. `TensorHandle` clones alias the same Python tensor. GPU tensors, other dtypes, implicit broadcasting, ownership transfer, DLPack, and zero-copy exchange are outside this slice. Python and PyTorch own object and tensor storage; CGx2 owns only the handle. Their allocations are foreign and are not charged to CGx2 regions. A CGx2 `no_gc` block cannot stop Python GC or PyTorch allocation.

`PythonBridge::probe_torch()` returns `TorchProbe { python_version, python_prefix, status }`. `TorchStatus::Available { torch_version }` means the selected interpreter imported `torch`, constructed a real CPU float32 `torch.Tensor`, and returned the expected scalar from both typed multiplication and matmul. `Unavailable` means the top-level `torch` package is absent; `Broken` retains an import, version, or capability error, plus the version when it could be read. A successful import or a module named `torch` alone is not enough for `Available`. The probe describes the interpreter linked into this build, not a Python executable selected later at runtime.

Run `cargo test -p cgx2-python-bridge --offline -- --nocapture`. To select a Python environment with real PyTorch, set `PYO3_PYTHON=/path/to/python` **before building** and rerun the command; that environment must also provide its matching development library. `PythonBridge::new()`, `python_version()`, and `python_prefix()` identify the interpreter actually linked to this build. On a system that has only a versioned runtime library such as `libpython3.14.so.1.0`, the link step needs the matching unversioned development-library link name (`libpython3.14.so`). A temporary symlink can supply it through `LIBRARY_PATH` without changing system files. The optional tests import the **real** `torch` package. If it is absent, they print `SKIPPED real PyTorch` notices and assert the unavailable path. With PyTorch installed they check creation, addition, multiplication, rectangular matmul, zero-sized output, result values, shape/dtype/device, and rejected integer tensors. A skipped test is a missing external acceptance gate, not evidence that real PyTorch works. This suite does not establish GPU behavior or the latency targets in `docs/validation-plan.md`. This is a host API: it dynamically calls importable Python objects, but does not add `.cg` source syntax or static safety for arbitrary Python code. Connecting this bridge to `.cg` imports remains later integration work.
