# CGx2 declared Python calls

`cgx2-foreign` provides Python-free declaration/error metadata and a bounded scalar worker by default. Enable `in-process` for the existing linked Rust host API over `cgx2-python-bridge`. It parses a UTF-8 JSON declaration (at most 65,536 bytes), then permits calls only by exact declared key. Parsing does not import Python. A call validates the key, positional count and types, keyword names and types, and the existing bridge depth/node/byte/keyword limits before importing a module or reading an attribute. Python execution is not transactional: a return conversion error may follow a successful call with side effects.

```json
{
  "version": 1,
  "calls": [
    {
      "key": "math.sqrt",
      "module": "math",
      "attribute": ["sqrt"],
      "positional": ["f64"],
      "keywords": {},
      "returns": "f64",
      "effect": "Foreign",
      "ownership": "copied"
    }
  ]
}
```

`module` is a dotted Python module name and `attribute` is a nonempty exact attribute chain. Each component and keyword name is an ASCII Python identifier; `key` is a nonempty dotted identifier used only to select the allowlist entry. Wildcards, unknown fields, unknown types, duplicate JSON fields or allowlist keys, mismatched return ownership, and direct Python evaluation or dynamic import targets (`builtins.eval`/`exec`/`compile`/`__import__`, `importlib.import_module`, and `runpy.run_module`/`run_path`) are rejected. The declaration is trusted configuration, not a sandbox for arbitrary allowed libraries. Positional arguments are required in the declared order. Declared keywords are optional; undeclared or repeated keywords fail before import. There are no defaults, variadics, or implicit numeric conversions in the boundary contract.

Shapes are `none`, `bool`, `i64`, `f64`, `string`, `bytes`, `list`, `tuple`, `dict`, and `python_handle`. Container shapes describe the outer type; nested values must still satisfy the bridge's recursive depth, node, and byte limits. A `copied` result must have the declared outer shape and contain no Python handles, including nested handles. A `python_handle` result uses return shape `python_handle` and preserves a PyO3 reference without copying the object. A handle argument aliases a Python object. Python and PyTorch own their objects, tensors, allocators, and GC; CGx2 regions and `no_gc` cannot constrain them. This API does not promise zero-copy exchange, a safe arbitrary-library ABI, or a GPU backend.

```rust
use cgx2_foreign::parse_declarations;
use cgx2_python_bridge::{BridgeValue, PythonBridge};

let registry = parse_declarations(r#"{"version":1,"calls":[{"key":"math.sqrt","module":"math","attribute":["sqrt"],"positional":["f64"],"keywords":{},"returns":"f64","effect":"Foreign","ownership":"copied"}]}"#)?;
let bridge = PythonBridge::new()?;
let result = registry.call(&bridge, "math.sqrt", &[BridgeValue::F64(9.0)], &[])?;
assert!(matches!(result, BridgeValue::F64(3.0)));
```

`Registry::to_json()` emits deterministic compact JSON for API tooling. `ForeignError::category()` gives `declaration`, `blocked_call`, `argument`, `module_unavailable`, `attribute_missing`, `python_exception`, or `return_conversion`. Python exceptions retain capped class, message, and traceback. A missing module or attribute is distinct from a Python exception or wrong return shape.

## `.cg` source adapter

The Wave 10 CLI adapter builds `SourceCall` entries from checked `foreign python fn` declarations and passes them through `registry_from_source`, which uses this crate's strict allowlist parser. `Registry::call_worker` validates the narrower source subset before process discovery or import and checks the copied return shape. `Registry::call` remains available with `in-process`. The source checker and CLI attach the call span and enforce `Foreign`/`ForeignCall`; this host crate remains the runtime boundary. `local:cpython` identifies the selected local CPython worker runtime, not a pinned package environment. The route does not load PyTorch or substitute for direct LibTorch.

Run `cargo test -p cgx2-foreign --offline`. Embedded CPython must have a matching development link library; `LIBRARY_PATH` can point at its directory. Run bridge tests separately to exercise `probe_torch()` against the linked interpreter. Real PyTorch absence is a skipped external capability gate.

## Declared copied float64 array

`f64_array_1d_copy_v1` is an additive top-level declaration shape for explicitly allowlisted Python-only functions. For example, an `array.array` round trip declares `{"module":"array","attribute":["array"],"positional":["string","f64_array_1d_copy_v1"],"returns":"f64_array_1d_copy_v1","effect":"Foreign","ownership":"copied"}` within a version 1 call entry. Pass `BridgeValue::F64Array1DCopyV1(Vec<f64>)`; the result is the same variant with a new CGx2-owned vector. A Python list is not a valid return buffer for this shape. Input failures have category `argument`; incompatible buffers have `return_conversion`; a Python callable that throws still has `python_exception`. The existing scalar/list/dict limits and return behavior are unchanged.

This is a Rust host declaration route. The `.cg` source compiler was outside this wave's ownership, so source syntax for this shape remains a later integration step. The allowlist restricts which function can receive the array; it does not guarantee that the library is pure or free of side effects.

## Scalar worker and packaging

`Registry::call_worker(&WorkerConfig, key, &[ScalarValue])` returns `WorkerOutput { value, provenance: PythonProvenance { executable, version, prefix } }`. It accepts only copied `I64`, `Bool`, and UTF-8 `String`, no keywords, one public `math` attribute or `builtins.str`/`builtins.bool`. Broader declarations remain usable by the opted-in in-process API, not by this worker. The CLI checker still enforces `runtime "local:cpython"`, `Foreign` effects, and `ForeignCall` capabilities. Validation runs before interpreter discovery/spawn. There is no array/handle source syntax or implicit coercion.

`WorkerConfig::default()` selects `CGX2_PYTHON`, otherwise the first executable `python3` on PATH. Set an absolute executable path for reproducible selection. Paths are resolved once; launch failure never selects another interpreter. The response must report CPython 3 and an executable whose canonical path matches the selected executable, with an absolute prefix and a version string. This is local **unpinned** provenance, not package/version attestation or a sandbox against a malicious selected interpreter/worker.

The audited, stdlib-only `src/worker.py` is embedded with `include_str!` in the Rust library and included by Cargo packaging. No separately installed helper or script is required. `cargo build -p cgx2-cli --offline` builds both `cgx2` and its existing native helper. `cargo install --path crates/cgx2-cli --offline` installs both binaries; keep them together for native-package dispatch. To explicitly replace the embedded script, set `CGX2_PYTHON_WORKER` to an existing absolute file (also exposed by `WorkerConfig.worker_path`). Missing files fail without fallback. Overrides are trusted administrator/test configuration.

The selected interpreter runs with `-I -S`: ignore `PYTHONPATH`, user site, script/current-directory module search, and site startup hooks. Only the selected interpreter's standard-library paths are searched. This preserves the existing math/builtins allowlist; it does not enable external Python packages. User/module stdout, including fd 1 writes, goes to `/dev/null`; stderr is discarded. A duplicated protocol descriptor carries only the result. One process handles one invocation; module state does not persist between calls.

### Protocol v1 and limits

All integers and lengths use big-endian encoding. Request: ASCII `CGX2PY01Q`, length-prefixed UTF-8 module and attribute, result tag, u32 argument count, then scalar values. Value tags: `i` + signed i64, `b` + byte 0/1, or `s` + u32 length + UTF-8 bytes. Response: `CGX2PY01R`, four length-prefixed strings (implementation, executable, version, prefix), then status 0 + scalar, 1 + module + Python failure, 2 + attribute + failure, 3 + failure, or 4 + actual return type. A failure is class/message/traceback strings. Every frame must end exactly after its payload. There is one frame per process, terminated by EOF and successful process exit.

Both frames are capped at 1,179,648 bytes. At most 4,096 arguments and 1,048,576 aggregate argument-string bytes are accepted; a returned string has the same byte cap. Declaration names are bounded by the 65,536-byte declaration budget. Provenance fields are at most 4,096 bytes (implementation 32). Failure class/message/traceback total at most 16,384 bytes (class 256, message 4,096). Invalid tags, bool encodings, UTF-8, lengths, truncation, trailing bytes, identity, shape, exit status, or excess output discard the entire response.

The deadline defaults to five seconds. API callers may select a positive `WorkerConfig.timeout` no greater than five seconds. Nonblocking pipes cover request writes, response reads, EOF, and child exit; each drain iteration is bounded so continuous output cannot starve the deadline. Every spawned child is waited for, and failed/timed-out children are killed before reaping. No reader/writer threads survive. The transport currently supports Linux x86_64/aarch64; other targets report an explicit unavailable error. This bounds protocol transport and call duration, not foreign CPU/RSS allocation. The audited worker never forks; a hostile administrator override is outside that contract.

Worker/discovery/protocol failures map to `declaration` (`CGR017`); module, attribute, Python exception, and typed-return failures retain their existing categories (`CGR018`–`CGR021`). CLI errors retain the first `.cg` call span. Native/LibTorch calls do not discover or start a worker. Workspace feature unification may build the host, but the CLI never uses PyO3-owning values; inspect the actual final binaries as described in the CLI evidence.

```sh
cargo test -p cgx2-cli -p cgx2-foreign -p cgx2-python-bridge --offline
cargo test -p cgx2-foreign -p cgx2-python-bridge --features cgx2-foreign/in-process --offline
cargo package -p cgx2-foreign --list --allow-dirty --offline
```
