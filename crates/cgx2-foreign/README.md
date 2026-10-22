# CGx2 declared Python calls

`cgx2-foreign` is a Rust host API over `cgx2-python-bridge`. It parses a UTF-8 JSON declaration (at most 65,536 bytes), then permits calls only by exact declared key. Parsing does not import Python. A call validates the key, positional count and types, keyword names and types, and the existing bridge depth/node/byte/keyword limits before importing a module or reading an attribute. Python execution is not transactional: a return conversion error may follow a successful call with side effects.

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

The Wave 10 CLI adapter builds `SourceCall` entries from checked `foreign python fn` declarations and passes them through `registry_from_source`, which uses this crate's strict allowlist parser. `Registry::call` still validates arguments before import and checks the copied return shape. The source checker and CLI attach the call span and enforce `Foreign`/`ForeignCall`; this host crate remains the runtime boundary. `local:cpython` identifies the linked local runtime, not a pinned package environment. The route does not load PyTorch or substitute for direct LibTorch.

Run `cargo test -p cgx2-foreign --offline`. Embedded CPython must have a matching development link library; `LIBRARY_PATH` can point at its directory. Run bridge tests separately to exercise `probe_torch()` against the linked interpreter. Real PyTorch absence is a skipped external capability gate.
