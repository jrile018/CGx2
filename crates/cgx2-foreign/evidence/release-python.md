# S1-05 pinned Python runtime delivery evidence

Date: 2026-10-23. Base: `0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
Branch: `riley/pinned-python-runtime`. This report and implementation are committed
together; the delivery commit is the commit containing this file (`git log -1`).
No merge or subsequent stage was started.

## Implemented and supported

- Additive Linux embedded `package_v1::PackageEnvironmentV1` and explicit
  `PackageAuthorizationV1`; strict versioned lock, platform/runtime/environment,
  distribution METADATA/dependency and SHA-256 source identity checks on every call.
- Exclusive isolated startup checks pins before initialization, explicitly chooses
  sealed stdlib paths and disables site/PYTHONPATH/bytecode startup. An alternative
  existing-embedder API has an explicit trusted startup boundary. Actual mapped
  `Py_GetVersion` image and C API version are checked for isolated initialization.
- Snapshot package loading and bounded copied scalars, strings/bytes, homogeneous
  lists/dicts, typed tuples and native-endian 1D finite float64 arrays. No package
  handles cross the boundary. Scoped import restoration and attached reference
  release cover success, Python exceptions and conversion failures.
- The legacy declaration/value/error types and scalar `-I -S` child remain
  unchanged. Default CLI normal dependency graph excludes PyO3. Native/LibTorch
  paths were not changed and have no new fallback.
- Operator-run real dateutil adapter/example and preparer, with explicit exact
  versions/environment supplied by the operator, sealed stdlib copy, no install.
  Documentation specifies capabilities, trust/limits, errors and S2/S3 handoffs.

This is **Rust library support**, not new `.cg` syntax, compiler lowering,
notebook behavior or general package ecosystem parity. The shared worker wire
contract remains documentation-only for this route.

## Files

`crates/cgx2-foreign`: `src/package_v1.rs`, small internal budget/strict-JSON reuse
in `src/call.rs`/`src/declaration.rs`, `src/lib.rs`, `Cargo.toml`,
`tests/package_v1.rs`, `tests/fixtures/pinned_demo/__init__.py`,
`examples/pinned_dateutil.rs`, `scripts/prepare_dateutil_lock.py`,
`scripts/test_prepare_dateutil_lock.py`, the pinned dateutil adapter/metadata,
`PINNED-PYTHON-V1.md`, `README.md` and this evidence directory.

`crates/cgx2-python-bridge`: `src/package_host_v1.rs`,
`src/package_loader_v1.py`, internal conversion reuse in `src/host.rs`,
`src/lib.rs`, `README.md`. No existing public enum variants or required fields
were changed. No new crate was added.

## Actual verification

All Cargo commands used these environment variables from the attached worktree:

```sh
export CARGO_BUILD_JOBS=2 RUSTUP_TOOLCHAIN=1.98.1
export CARGO_TARGET_DIR="$PWD/target/release-python"
export TMPDIR="$PWD/target/tmp-python"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
```

| Command | Exit | Result |
| --- | --- | --- |
| `cargo test -p cgx2-foreign -p cgx2-python-bridge --all-features --offline -- --nocapture` | 0 | 56 passed, 0 failed, 0 ignored; existing optional PyTorch tests report real torch absent and execute their skip paths |
| `cargo clippy -p cgx2-foreign -p cgx2-python-bridge --all-targets --all-features --offline -- -D warnings` | 0 | Warnings denied; clean |
| `cargo test -p cgx2-foreign -p cgx2-python-bridge --offline` | 0 | 14 passed, 0 failed, default Python-free profile |
| `cargo fmt -p cgx2-foreign -p cgx2-python-bridge --check` | 0 | Owned formatting clean |
| `git diff --check` | 0 | Clean |
| `PYTHONDONTWRITEBYTECODE=1 TMPDIR=$PWD/target/tmp-python python3 -m unittest discover -s crates/cgx2-foreign/scripts -p test_prepare_dateutil_lock.py -v` | 0 | 2 local preparer tests, no real package imports |
| `cargo tree -p cgx2-cli --offline -e normal` plus assertion `"pyo3" not in tree` | 0 | Default CLI dependency graph excludes PyO3 |
| `cargo run -p cgx2-foreign --all-features --example pinned_dateutil --offline -- --identity` | 0 | Compiled runner/trusted identity probe; output in `runtime-identity-v1.json` |
| `PYTHONDONTWRITEBYTECODE=1 python3 crates/cgx2-foreign/scripts/prepare_dateutil_lock.py --help` | 0 | Executable supplied-environment handoff |

Logs are private ignored artifacts in `target/release-python/owned-tests.log`,
`owned-clippy.log`, `default-tests.log`, `default-cli-tree.log`,
`identity-probe.log`, `dateutil-preparer-help.log`. The environment was Linux
x86_64, Rust 1.98.1, linked CPython 3.14.4 / prefix `/usr`; the recorded interpreter
and actual libpython paths/hashes are in `runtime-identity-v1.json`. Its empty
`stdlib` is intentional probe output, not an executable isolated package lock.

New tests cover wrong schema/platform/env/interpreter/library/version/prefix,
runtime/metadata/source/dependency hashes, distribution version/closure,
undeclared module/key/grant/keyword, argument arity/type/limits, missing callable
attribute and actual Python signature mismatch, result mismatch, exceptions,
hostile objects/non-string dictionary keys/cycles/deep/oversized output, copied
arrays, dependency drift between calls, source path traversal/symlink rejection,
bytecode bypass, standard tree drift, namespace-cache rejection, ambient cached
dependency rejection for normal/dynamic imports, host import-state restoration,
immediate destructor counts and isolated startup without site/PYTHONPATH hooks.

Initial development checks found one Clippy `for_kv_map` warning; it was fixed
and the final warnings-denied command passed. No test result is fabricated.

## Limitations and external acceptance

| Gate | Status | Required handoff |
| --- | --- | --- |
| Real pinned python-dateutil call | blocked/unrun | Supply reviewed exact python-dateutil/six and matching CPython development environment; run commands in `PINNED-PYTHON-V1.md`, review/authenticate generated lock and retain output |
| `.cg` source package call and offline package manifest integration | unrun / S2-S3 dependency | Source owner must authenticate environment locks, lower admitted copied types and enforce `Foreign`/`ForeignCall` grants; run real package fixture from source |
| Leak-enabled global foreign/memory acceptance | blocked/unrun external gate | Capable leak-enabled host and full integrated source/lifetime corpus; destructor/reference tests here are bounded fixture evidence |
| Calibrated runtime/bridge performance | unrun | Quiet host, complete startup/hash/import/copy/release accounting and applicable baselines; no benchmark run on the active multi-chat host |
| Real PyTorch | unavailable in linked interpreter | Existing optional tests explicitly skip; no tensor or native inference acceptance claim from this delivery |

This profile runs trusted Python without OS confinement, timeout/cancellation,
resource quotas, callbacks or retained package sessions. Python effects are not
transactional; result conversion can fail after effects. Python's own allocations
and GC are not CGx2 region budgets. Already initialized embedders retain their
startup/cache trust boundary. Isolated startup has an unsafe exclusive-startup
precondition; on initialization failure the application must terminate rather
than attempt recovery. Runtime artifacts must remain immutable during checking
and execution. Dynamic native-loader dependencies remain trusted host inputs.
General extension modules/package resources and full marker/version solving are
unsupported; v1 requires all named Requires-Dist dependencies conservatively.

## Coordinator dependency reconciliation

Owned `Cargo.toml` enables serde's already available `derive` feature and adds
optional `sha2 = "=0.10.9"` to `cgx2-foreign`'s `in-process` feature. All versions
were already in the base offline registry/lock. Cargo generated exactly one root
lock dependency-list addition (`"sha2"` under cgx2-foreign); it was discarded
before commit as required. The coordinator must regenerate/reconcile that owned
manifest dependency in its root Cargo.lock before using `--locked`. No root
Cargo manifest/lock change is included in this delivery. The preexisting
untracked `.serena/` directory was left outside the commit.
