# Build profiles

The default feature set exposes only Python-free `BridgeError`, `PythonFailure`, and limit metadata. It neither builds PyO3 nor needs Python development libraries. Enable `in-process` explicitly for `PythonBridge`, `BridgeValue`, `PyHandle`, `TensorHandle`, copied f64 arrays, and the existing host APIs below:

```sh
cargo test -p cgx2-python-bridge --features in-process --offline
cargo run -p cgx2-python-bridge --features in-process --release --example numeric_array_bench --offline
```

This profile deliberately links CPython and requires its development linker library (for example `libpython3.14.so`) and runtime loader search path. `PYO3_PYTHON=/absolute/python` selects the linked interpreter at build time using PyO3's usual configuration. This setting is separate from the CLI worker's runtime `CGX2_PYTHON` selection. Arrays and explicit handle lifetimes retain their existing behavior. Ordinary CLI calls use the isolated scalar route documented in `cgx2-foreign`, with no in-process fallback.

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

Run `cargo test -p cgx2-python-bridge --features in-process --offline -- --nocapture`. To select a Python environment with real PyTorch, set `PYO3_PYTHON=/path/to/python` **before building** and rerun the command; that environment must also provide its matching development library. `PythonBridge::new()`, `python_version()`, and `python_prefix()` identify the interpreter actually linked to this build. On a system that has only a versioned runtime library such as `libpython3.14.so.1.0`, the link step needs the matching unversioned development-library link name (`libpython3.14.so`). A temporary symlink can supply it through `LIBRARY_PATH` without changing system files. The optional tests import the **real** `torch` package. If it is absent, they print `SKIPPED real PyTorch` notices and assert the unavailable path. With PyTorch installed they check creation, addition, multiplication, rectangular matmul, zero-sized output, result values, shape/dtype/device, and rejected integer tensors. A skipped test is a missing external acceptance gate, not evidence that real PyTorch works. This suite does not establish GPU behavior or the latency targets in `docs/validation-plan.md`. This is a host API: it dynamically calls importable Python objects, but does not add `.cg` source syntax or static safety for arbitrary Python code. Connecting this bridge to `.cg` imports remains later integration work.

## Checked copied numeric arrays

`BridgeValue::F64Array1DCopyV1(Vec<f64>)` owns a contiguous Rust float64 vector. The bridge checks length with `len.checked_mul(8)` and finiteness before entering Python, then creates a separate Python `array('d')` and copies the values into its buffer while attached to CPython. `PythonBridge::to_f64_array_1d_copy_v1(handle, input_bytes)` checks a returned buffer's rank (exactly one), native-endian float64 format, item size, C contiguity, length, and finiteness before returning another owned Rust vector. Buffer references are acquired, copied, and released within one GIL attachment. The result cannot retain a borrowed Python pointer. Python lists and noncontiguous or foreign-endian views fail with `BridgeError::TypeConversion`.

Each declared input or output holds at most 1,048,576 elements and 8,388,608 payload bytes. The caller supplies the still-live CGx2-owned copied input byte count to the return conversion; the total of CGx2-owned input and output copied payload, including ordinary strings and bytes, is capped at 16,777,216 bytes. The generic 4,096-node and 1 MiB string/bytes limits remain separate. Python's own objects, temporary allocations, and GC are outside CGx2's memory budget; this route makes no zero-copy or process-wide memory guarantee. `array('d')` and compatible native NumPy float64 buffers work; arbitrary Python numerical types and device buffers are not automatically compatible.

`examples/numeric_array_bench.rs` compares 1,000 embedded calls to scalar `operator.add`, a copied 256-element Python list, and a copied 256-element `array('d')`. On this host with linked CPython 3.14.4 (GCC 15.2.0), one run measured 0.18, 11.36, and 1.60 microseconds per call respectively. The test interpreter also has NumPy 2.3.5. These numbers include conversion and Python dispatch for these call shapes on this host; they are not cross-host latency guarantees. Run with `LIBRARY_PATH=/usr/lib/python3.14/config-3.14-x86_64-linux-gnu cargo +1.98.1 run --release -p cgx2-python-bridge --features in-process --example numeric_array_bench --offline` here.
