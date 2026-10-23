# Release core command delivery (S1-01)

The product CLI uses the compiler library directly. A separately installed
`cgx2c`, Cargo, a source checkout, and Python are unnecessary for native builds.
`rustc` must be available to build an executable; it is unnecessary for emission
or API queries. Existing interpreter, package-helper, memory, and formatter
commands keep their existing behavior.

```sh
cgx2 build FILE.cg --out PROGRAM
cgx2 build PACKAGE_DIRECTORY --out PROGRAM --release
cgx2 emit FILE.cg --target rust --out FILE.rs
cgx2 emit FILE.cg --target python --out FILE.py
cgx2 emit PACKAGE_DIRECTORY --target rust --out FILE.rs
cgx2 emit PACKAGE_DIRECTORY --target python --out PACKAGE.pyz
cgx2 api --json FILE.cg
cgx2 api --json PACKAGE_DIRECTORY
```

An explicit `--out` is required. Its parent must exist. Existing regular files
require `--force`; source files, package inputs, hard links to inputs, symlinks,
and directories reject. Generation and compilation finish in a private directory
beside the destination before publication. Replacement uses atomic rename;
creation uses a hard link that fails if the destination appeared concurrently.
Failures preserve the previous destination and remove staging files. Checked
packages revalidate their source/manifest hashes before publication.

Native compilation defaults to the debug profile (`opt-level=0`); `--release`
selects `opt-level=3` for both the generated program and any embedded memory
runtime. `--rustc PATH` overrides `CGX2C_RUSTC`, which otherwise defaults to
`rustc`. Emission rejects build-only flags. All commands return 0 on success,
1 on error. Build metadata is printed only after installation succeeds.

`--diagnostic-format=json` emits error JSON Lines on stderr. Source locations
are zero-based, half-open UTF-8 byte spans. Package source diagnostics use
package-relative file names. API output remains the existing deterministic module
or package index schema, including checked transitive effect summaries and public
visibility. Legacy `api PATH --json` remains supported.

Python packages contain the existing emitter's individual `.py` sources in a
deterministic, uncompressed standard zipapp. `--out FILE.pyz` is required for this
case. Run with `python3 FILE.pyz`; inspect/extract with Python's `zipfile` module.
Members have fixed 1980-01-01 timestamps, lexical ordering, UTF-8 names, and CRC32
checksums. No Python process is used during emission. The source archive allows
the entire multi-module output to be atomically replaced like a single file.
The native/Rust and portable Python profiles remain those admitted by the existing
emitters. Unsupported memory/foreign/package constructs still reject explicitly.

## Verification and executable evidence

See [the recorded results](release-core-evidence.md) and
[`scripts/verify_release_core.py`](scripts/verify_release_core.py). The acceptance
runner copies installed binaries and checked fixtures into `/tmp`, runs there,
and records each argv, exit code, stdout/stderr, deterministic output comparisons,
native/Rust/Python results, error preservation, Unicode spans, explicit Python
worker selection, and native ELF dependency inspection. It does not benchmark.

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-core"
export TMPDIR="$PWD/target/tmp-core"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo install --offline --path crates/cgx2-cli --root "$TMPDIR/install" --force
python3 crates/cgx2-cli/scripts/verify_release_core.py \
  --binary "$TMPDIR/install/bin/cgx2" --output "$TMPDIR/installed-core.json"
```

Native/package run still uses the installed sibling `cgx2-native` on Linux.
New build/emit/API routes need only `cgx2`; the integration test also checks an
isolated copied product binary without either compiler or native helpers.

## Stage 2 and external handoff

The compiler-owned [consumer fixture](../cgx2c/tests/execution_consumer.rs)
checks `tests/fixtures/release-core/pipeline.cg` with `check_module`, proves its
kernel summaries have no effects/capabilities, owns the input `Vec<i64>` buffers
at the Rust boundary, and compares the current interpreter with policy-library
map/zip results and overflow/length failures. Its exact reviewed entry points in
`crates/cgx2-execution-policy/src/lib.rs` are:

- `execute_map(input: &[i64], multiplier: i64, addend: i64, policy: Policy,
  caps: &Capabilities) -> Result<Execution, ExecuteError>` computes checked
  `input[i] * multiplier + addend`.
- `execute_zip_map(left: &[i64], right: &[i64], addend: i64, policy: Policy,
  caps: &Capabilities) -> Result<Execution, ExecuteError>` computes checked
  `left[i] * right[i] + addend`.
- `execute_zip_map_cancellable` additionally takes `&AtomicBool` for cancellation.

`Policy::Auto` with default capabilities stays sequential without valid profile
evidence. S2 must lower admitted source collections to these calls, retain owners
for the call duration, and exclude managed aliases, hidden effects, unsupported
memory activity, and arbitrary kernel semantics. This fixture is a library consumer
contract; collection source lowering and automatic GPU selection are not implemented.

Required later command integration remains explicit: `test` needs discovery and
properties conventions; `verify` needs the reviewed proof profile and real solver
routing; `notebook` needs the notebook command/runtime/session integration; `bind`
needs the reviewed foreign lock/environment command integration. None of these
commands is advertised as installed by this delivery. Variants, generics, full
memory/foreign lowering, and new AST/type/value forms belong to S2.

Real third-party Python packages, broader direct LibTorch/source support, supported
GPU devices, leak-enabled hosts, live trusted solvers, isolated benchmark hosts,
and human/model studies remain later acceptance work. This delivery's Python test
uses stdlib `math.isqrt`. The existing pinned CPU float32 LibTorch package was also
executed successfully; see the recorded boundary evidence for artifact pins and
actual process maps. The native boundary script accepts
`--real-f32-package /ABS/REVIEWED_PACKAGE`; run it only with actual pinned artifacts.
Follow the finite completion plan's S4 procedures for host/study gates; these
conformance checks do not close broader support or empirical gates.
