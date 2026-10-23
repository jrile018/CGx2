# Pinned Python packages v1

`package_v1` is an additive Rust library API behind `in-process`. It supports
explicit, copied calls into pinned pure-Python packages on Linux CPython. It
does not add `.cg` package syntax or change `Registry`, `ValueShape`, `ScalarValue`,
the scalar child protocol, or any native/LibTorch route. Package extensions,
package data resources, namespaces spanning distributions, callbacks, retained
handles, automatic installation and persistent package state are outside v1.
Virtual modules constructed by pinned source (for example `six.moves`) can work;
they are not independent filesystem artifacts or callable declaration targets.

## Lock and grants

`PackageEnvironmentV1::from_json(bytes)` accepts at most 1 MiB of strict JSON.
Unknown and duplicate fields, including duplicate callable/keyword map keys,
reject. `schema_version` is exactly `1`. There are no implicit environment,
interpreter, callable, dependency or signature defaults. The operator must review
and authenticate the lock itself, e.g. pin its SHA-256 in a future package lock.
Self-reported hashes in an unauthenticated lock are not a trust root.

The lock explicitly records:

- `environment_id`, `os`, `arch`; `runtime.interpreter` and `runtime.library`
  each have canonical absolute `path` and lowercase hexadecimal `sha256`.
  The library must be the actual mapped image containing `Py_GetVersion`,
  found through `/proc/self/maps`.
- Exact `runtime.version` (the complete CPython version string), canonical
  `prefix` and `base_prefix`, and `runtime.stdlib` directory tree pins.
  Non-Linux mapping inspection fails explicitly. Runtime/dynamic-loader/system
  dependencies are trusted host infrastructure; this is not a complete ELF or
  operating-system dependency seal.
- Each distribution's exact normalized `name`, `version`, canonical `root`,
  relative `metadata.path` with SHA-256, explicit `dependencies`, and `sources`.
  Each source lists `module`, relative `.py` `path`, SHA-256, and `is_package`.
  Every submodule has a pinned package parent. Duplicate modules, split top-level
  ownership, traversal and symlink components reject. Missing files fail closed.
  METADATA Name/Version must match exactly. Every named Requires-Dist row must
  appear in `dependencies`, including conditional/extra requirements: v1 has no
  marker, extras or version-range resolver. Compatible exact versions are the
  lock author's responsibility. All dependencies must exist in the lock.
- Each entry in `calls` identifies a pinned `module`, one public `attribute`,
  `positional`, `keywords`, `returns`, `effect: "Foreign"`,
  `capability: "ForeignCall"`, `ownership: "copied"`.
  Every declared keyword is required. `CopiedTypeV1` is tagged JSON, e.g.
  `{"kind":"list","items":{"kind":"i64"}}`; tuples have an array of
  `items`, dicts have one value type and UTF-8 keys. Scalars use
  `none`, `bool`, `i64`, `f64`, `string`, `bytes`; the top-level numeric array kind
  is `f64_array1d_copy_v1`. Floats must be finite. Boolean and i64 are distinct.

`PackageAuthorizationV1 { environment_id, allowed_calls }` is the application's
explicit grant. The environment ID and exact key must match before execution.
This object is an allowlist and does not provide OS confinement. Approved Python
code can perform arbitrary effects; it must be trusted for the granted operation.

```rust,ignore
let lock = PackageEnvironmentV1::from_json(&authenticated_lock_bytes)?;
// At process startup, before any other CPython client/thread:
let bridge = unsafe { lock.start_isolated()? };
let grant = PackageAuthorizationV1 {
    environment_id: expected_environment_id,
    allowed_calls: ["dateutil.isoformat".to_owned()].into(),
};
let copied = lock.invoke(&bridge, &grant, "dateutil.isoformat",
    &[BridgeValue::String("2024-02-29T12:34:56Z".into())], &[])?;
```

## Embedded startup and import guarantees

`unsafe PackageEnvironmentV1::start_isolated()` verifies platform and every
runtime, stdlib, distribution and source pin before initializing CPython. It uses
`PyConfig_InitIsolatedConfig`, disables site/user-site/environment/bytecode writes,
and sets the exact pinned stdlib search directories, including for startup's
encodings import. It checks the mapped image and actual C API version before
initialization, then verifies executable/version/prefix identity. Already
initialized interpreters reject; there is no fallback to ordinary initialization.
An unsafe startup precondition requires exclusive application startup before any
other CPython/PyO3 use. It does not finalize the process-global interpreter.

Isolated startup requires a cache-free stdlib snapshot (even an empty
`__pycache__` directory rejects). Directory hashes use sorted UTF-8 relative
filenames, NUL, lowercase file SHA-256, LF. `__pycache__`, `site-packages`,
`dist-packages` are excluded from the general tree hash; non-cache `.pyc/.pyo`,
symlinks and special files reject. Trees are bounded to 16,384 entries and
256 MiB of file bytes. The preparer copies regular file symlink targets as
bytes into the private snapshot; directory symlinks reject. Freeze runtime
files/directories against concurrent mutation. No filesystem hash preflight can
undo already executed native loader code or defend a malicious concurrent host.

Alternatively, `invoke(&existing_bridge, ...)` supports a trusted, already
initialized embedder. Initialization/site hooks and previously loaded stdlib
code then belong to the embedding application's trust boundary. This form
guarantees package preflight before the scoped loader's package initialization;
it cannot retroactively prevent startup imports. `PythonBridge::new()` retains
its existing startup behavior. Never use it to claim isolated startup.

Every invocation rechecks runtime/environment identity, argument types/budgets,
all metadata/source hashes and dependency pins before any pinned module import.
The loader compiles the verified byte snapshots instead of reopening package
files or consulting package bytecode. It rejects preloaded pinned namespaces
and stdlib shadowing. Package imports and normal `importlib.import_module` calls
reject undeclared third-party dependencies even if ambient cached modules exist.
The scoped standard-library loader ignores bytecode and restricts filesystem
origins to hashed stdlib roots. Built-in/frozen modules remain part of CPython.
These resolution rules do not sandbox deliberately hostile approved code.

The API admits one non-reentrant package call at a time (busy is an error), and
requires a trusted host with no concurrent clients mutating Python import state.
It restores host path/meta-path/bytecode/importlib settings and removes pinned
namespace entries on success and Python failure. Each call imports fresh package
state. Arbitrary Python mutations, I/O and external effects are not rolled back.
There is no embedded deadline, forced cancellation or RSS/CPU confinement.

The existing `Registry::call_worker` remains a separate one-shot scalar child
using `-I -S`. It neither loads this API nor accepts these locks, containers or
arrays. The default CLI feature graph remains Python-free. This new package
route explicitly links CPython; neither route substitutes for native LibTorch.

## Copies, limits, errors and cleanup

Strings/bytes, homogeneous recursive lists/dicts, fixed heterogeneous tuples and
one-dimensional native-endian finite float64 arrays copy into owned Rust values.
No handles, arbitrary objects, subclasses, cycles, custom serializers or borrowed
Python buffers cross this boundary. The bridge does not use pickle/JSON for
package values or call a returned object's `str`, iterator or reducer to copy it.
Declared array conversion requests Python's buffer protocol; buffer-provider
hooks, like the approved callable itself, execute as foreign code.

Existing traversal limits apply: depth 16 (root depth 0), 4,096 nodes, 1 MiB of
UTF-8/string-key/byte payload and 128 keywords per input/result. Arrays are
top-level only, at most 1,048,576 elements/8 MiB each. Combined live copied
input and output payload is at most 16 MiB, checked before output copying.
Source snapshots are separate: at most 4,096 modules, 1 MiB per source/metadata
file and 16 MiB total source bytes. Streaming runtime artifact hashing is capped
at 256 MiB per file. These are traversal/copy limits, not Python allocation or
process-wide resource limits. Python allocation/GC remains foreign to CGx2.

`PackageErrorV1` distinguishes `Manifest`, `Identity`, `Authorization`, `Argument`,
`Result`, `Limit`, and `Python(BridgeError)`. Python failures preserve the existing
bounded exception class/message/traceback record (16,384 bytes retained total).
Actual Python arity/attribute/callability failures become TypeError/AttributeError;
the declared allowed signature is checked before import, Python's actual callable
signature is checked during invocation. Return mismatch may follow effects.
Python formatting/foreign execution itself is not allocation- or time-bounded.

Temporary PyO3 references and buffers drop while attached on successful and failed
calls. The API retains no package handles. Tests count immediate destructors for
rejected returned objects; they do not establish a process-wide leak guarantee.
Cycles created internally by package code still follow Python's GC rules.

## Real python-dateutil handoff (external, not fixture evidence)

Supply an already installed compatible CPython development/runtime environment,
exact reviewed versions of `python-dateutil` and `six`, readable `/proc`, and
write space for a private stdlib snapshot. No command below installs packages.
Select the interpreter at build time; runtime environment variables cannot switch
the linked CPython image. The adapter calls real `dateutil.parser.isoparse` and
copies `datetime.isoformat()` text; datetime objects themselves are unsupported.

```sh
export CARGO_BUILD_JOBS=2 RUSTUP_TOOLCHAIN=1.98.1
export CARGO_TARGET_DIR="$PWD/target/release-python" TMPDIR="$PWD/target/tmp-python"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
export PYO3_PYTHON=/absolute/supplied/python
# Set LIBRARY_PATH if its matching development link library requires it.
cargo build -p cgx2-foreign --all-features --example pinned_dateutil --offline
"$CARGO_TARGET_DIR/debug/examples/pinned_dateutil" --identity > "$TMPDIR/runtime.json"
"$PYO3_PYTHON" crates/cgx2-foreign/scripts/prepare_dateutil_lock.py \
  --runtime-identity "$TMPDIR/runtime.json" --environment-id reviewed-dateutil-env \
  --dateutil-version EXACT_REVIEWED_VERSION --six-version EXACT_REVIEWED_VERSION \
  --stdlib-snapshot "$TMPDIR/dateutil-stdlib" --output "$TMPDIR/dateutil.lock.json"
# Review/authenticate the resulting paths, versions and hashes before invocation.
"$CARGO_TARGET_DIR/debug/examples/pinned_dateutil" \
  "$TMPDIR/dateutil.lock.json" reviewed-dateutil-env
```

The stdlib snapshot destination must not already exist. Preparation imports only
trusted stdlib/metadata tooling; it does not import dateutil or six. `--identity`
is a separate trusted-host probe and does not claim isolated startup. The actual
runner uses isolated startup and checks the exact string
`2024-02-29T12:34:56+00:00`. Persist the lock, candidate/toolchain identity and
runner output for external acceptance. Missing packages or incompatible layouts
remain blocked/unrun; fixture tests cannot close real-package or source gates.

## S2/S3 integration instructions

Source owners must add an explicit versioned package-environment declaration and
authenticate its lock from `cgx2.toml`/the reviewed package-lock extension. Keep
legacy `runtime "local:cpython"` scalar behavior conservative. Check public
`Foreign` effects and `ForeignCall` capabilities transitively; bind checked calls
to the exact environment/key grant. Lower admitted source types into copied
`BridgeValue` representations, never opaque addresses/handles or unreviewed JSON
wire values. S1 has not selected the shared wire schema or extended frozen enums.

Choose isolated startup in an explicit Python host, or document an existing
trusted embedder's startup policy. Do not link this profile into the default
native CLI or introduce implicit native-to-Python fallback. Source owners must
still implement diagnostics and `.cg` acceptance, offline packaging, lifetime and
failure tests. S3 must run the real pinned dateutil fixture from `.cg`; S4 must
run leak-enabled audits and quiet-host measurements with startup, hashing,
imports, copies and cleanup included. No performance/release gate is closed here.
