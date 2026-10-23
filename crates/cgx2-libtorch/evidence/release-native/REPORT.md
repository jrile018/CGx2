# S1-04 native SDK and direct CPU inference delivery

Date: 2026-10-23. Base: `0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
Branch: `riley/release-native-sdk`. This report and its implementation are one
delivery commit; obtain its identifier with `git log -1 --format=%H` after
checkout (also reported in the delivery message). No merge or follow-on stage.

## Implemented scope

| Owned files | Result |
| --- | --- |
| `cgx2-ffi-core/{Cargo.toml,src/lib.rs,README.md}` | Dependency-free portable schema 2: fixed-width copied values, explicit Boolean and float bits, bounded homogeneous lists, errors, validated declarations and session-ID handle reference contract. Default ceilings: 1 MiB, 4096 nodes, depth 32. No generic wire decoder or worker registry. |
| `cgx2-bind/src/{lib.rs,sdk_v2.rs}`, examples and specification | Strict additive lock schema 3; exact native SDK v2 and tensor inference v4 profiles; host/hash/export/layout checks; bounded artifact read even if a file grows. Schemas 1/2 and previous layouts preserved. Offline fixture package renderer. |
| `cgx2-ffi-native/src/sdk_v2.rs`, `sdk_v2.h`, tests/examples | Single-call copied variable-length bytes/UTF-8, limits/invalid UTF-8, metadata pinning and retained inode, release-once unique ownership, immutable final-alias sharing, cross-adapter alias rejection for the same DSO inode, failure-output cleanup and caller-unwind cleanup. Calling-thread-only, no callback retention. |
| `cgx2-libtorch/src/lib.rs`, facade, preparation, tests/example | Existing finite f32 creation/matmul/info/copy plus v4 exact-shape addition and ReLU; two-layer CPU inference with independently computed scalar reference. Existing ABI v1 and matrix APIs v2/v3 stay intact. `--inference` writes schema 3 with full pinned ELF closure. |
| SDK-V2.md, SDK-LOCK-SCHEMA3.md, INFERENCE-V4.md and reproduce scripts | Exact contracts, offline examples, supported scope and concrete S2/S3 handoffs. |

The new SDK is **Rust/library-only**. No `.cg` compiler/types, notebook, Python
or worker integration changed. Existing source scalar native and f32 matmul
profiles remain on their old interfaces. Portable lists/scalars and worker IDs
are shared representations, not newly supported native dispatch signatures.
Native SDK dispatch supports bytes/UTF-8 and its typed i64 resource facade.
No autograd, GPU, arbitrary rank, model-file import or zero-copy was added.

## Verification and actual evidence

| Check | Status / exit | Evidence kind |
| --- | --- | --- |
| Four owned packages, offline all-feature tests | passed / 0: **44 passed, 0 failed, 6 ignored** | Rust plus generated C/C++ fixtures; six external tests still ignored in default suite |
| All-target/all-feature Clippy, `-D warnings` | passed / 0 | Owned package lint/build |
| Owned `cargo fmt --check`, `git diff --check` | passed / 0 | Formatting/patch checks |
| Native SDK package renderer, bind check, roundtrip client | passed / 0 | Synthetic C++ SDK fixture: `Utf8("héllohéllo"); retained_handle=42` |
| C++ facade configure/build | passed / 0 | **Real installed LibTorch 2.11.0**, g++ 15.2.0, C++11 ABI 1 |
| `--inference` package preparation and bind check | passed / 0 | Real copied facade/dependency closure with hashes |
| Real pinned two-layer inference | passed / 0 | Four cycles, CPU/f32, output `[-6.5, 8.5]`, independent scalar f32 reference, absolute tolerance 1e-5; no mapped libpython |
| Existing real f32 matrix reference/lifetime test | passed / 0 | Two cycles using the new package; v3 numerical/lifetime regression |
| Real inference CLI example | passed / 0 | `device=cpu dtype=1 shape=[2, 1] output=[-6.5, 8.5]` |
| C++ SDK fixture ASan/UBSan, detect_leaks=0 | passed / 0 | 100 ordinary plus 100 exception-output create/release cycles; **not leak evidence** |
| Same fixture, detect_leaks=1 | **blocked / 1** | LeakSanitizer fatal: does not work under ptrace. Log retained. |
| Quiet-host performance, full native/LibTorch leak gate, independent safety audit, GPU/autograd and source integration | **unrun** | No result claimed |

Negative fixtures cover wrong hash/target/ABI/identity/version/API/symbol/layout,
schema downgrade, missing closure, overreported and excessive output, invalid
UTF-8, C++ exception before/after owned output, Rust panic containment inside C,
duplicate release, terminal failed release, returned handle aliases (including
independent adapters), wrong shape/runtime and replaced artifact inode. Tensor
tests drop host input buffers before inference and retain handles past adapter
drop. Finite observations do not prove arbitrary native memory safety.

An initial owned test failed because its old unknown-schema case used newly
allocated schema 3; it now uses 99. A first real debug attempt exposed the old
schema-2-only opener check, which was corrected. A later debug real run was
interrupted (exit 130) because repeated cryptographic verification is slow in
debug; the complete release-mode run passed. The initial owned-test and interrupted-debug logs are retained.
Initial package preparation rejected ambiguous `libgomp.so.1`; explicit
selection of the installed Torch copy resolved it. No verification was weakened.
CMake also emitted its preexisting optional kineto-library warning; the actual
Torch target configured, linked and passed runtime checks.

## Exact commands/environment

All Cargo commands used:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-native" TMPDIR="$PWD/target/tmp-native"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test --offline --all-features -p cgx2-ffi-core -p cgx2-ffi-native -p cgx2-bind -p cgx2-libtorch
cargo clippy --offline --all-targets --all-features -p cgx2-ffi-core -p cgx2-ffi-native -p cgx2-bind -p cgx2-libtorch -- -D warnings
cargo fmt --check -p cgx2-ffi-core -p cgx2-ffi-native -p cgx2-bind -p cgx2-libtorch
git diff --check
```

`rustc 1.98.1 (48a229cea 2026-09-01)`. Real build/preparation used:

```sh
cmake -S crates/cgx2-libtorch/facade -B target/native-facade-build \
  -DCMAKE_PREFIX_PATH=/home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch \
  -DCMAKE_CXX_COMPILER=/usr/bin/x86_64-linux-gnu-g++-15 \
  -DCGX2_EXPECT_TORCH_VERSION=2.11.0 -DCMAKE_BUILD_TYPE=Release
cmake --build target/native-facade-build --config Release -j2
python3 crates/cgx2-libtorch/prepare_pinned.py \
  --facade "$PWD/target/native-facade-build/libcgx2_libtorch_facade.so" \
  --libtorch-root /home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch \
  --build-dir "$PWD/target/native-facade-build" --package "$PWD/target/native-real-package" \
  --version 2.11.0 --compiler /usr/bin/x86_64-linux-gnu-g++-15 --cxx11-abi 1 \
  --torch-cxx-flags='' --build-command='cmake --build target/native-facade-build --config Release -j2' \
  --lib-dir /usr/lib/x86_64-linux-gnu \
  --select-dependency=libgomp.so.1=/home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch/lib/libgomp.so.1 \
  --inference
cargo run --release --offline -p cgx2-bind --bin cgx2-bind -- check "$PWD/target/native-real-package"
LD_LIBRARY_PATH="$PWD/target/native-real-package" CGX2_LIBTORCH_REAL_PACKAGE="$PWD/target/native-real-package" \
  cargo test --release --offline --all-features -p cgx2-libtorch --test inference \
  real_pinned_two_layer_inference_no_python -- --ignored --exact --nocapture
LD_LIBRARY_PATH="$PWD/target/native-real-package" CGX2_LIBTORCH_REAL_PACKAGE="$PWD/target/native-real-package" \
  cargo test --release --offline --all-features -p cgx2-libtorch --test matrix_f32 \
  real_pinned_float32_reference_and_cycles -- --ignored --exact --nocapture
LD_LIBRARY_PATH="$PWD/target/native-real-package" cargo run --release --offline -p cgx2-libtorch \
  --example infer_cpu -- "$PWD/target/native-real-package"
```

Native fixture commands are in SDK-LOCK-SCHEMA3.md/reproduce-sdk.sh and their
actual outputs are retained here. Sanitizer commands:

```sh
c++ -std=c++17 -g -fsanitize=address,undefined -fno-omit-frame-pointer \
  crates/cgx2-ffi-native/tests/fixtures/sdk.cpp crates/cgx2-ffi-native/tests/fixtures/sdk_asan.cpp \
  -o target/release-native/sdk-asan
ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 target/release-native/sdk-asan # blocked, exit 1
ASAN_OPTIONS=detect_leaks=0:halt_on_error=1 target/release-native/sdk-asan # pass, exit 0
```

Summary JSON hashes the changed implementation; copied binding/closure/build
evidence pins the real artifacts. Binaries remain in the private ignored target,
not Git. Locks use local absolute host paths; another host must regenerate and
review them, not reuse them as fresh runtime evidence.

## Integration and external handoffs

- Coordinator must reconcile `cargo-lock-additions.patch`: new std-only
  `cgx2-ffi-core`, bind → core, native → bind/core, and native test use of existing
  pinned serde_json/sha2. No new registry version was installed. Generated root
  Cargo.lock edits were discarded; the root manifest/lock are not committed.
- Run `sh crates/cgx2-ffi-native/reproduce-sdk.sh --leaks` on a non-ptrace host;
  this covers the fixture ABI only. Full release memory acceptance still needs
  the supported source/runtime/real-library corpus and independent audit.
- `reproduce-inference.sh` accepts pinned installed-root/compiler/version/ABI
  environment variables and explicit dependency-selection arguments. It builds,
  prepares, checks and reruns both real tests and the CLI example offline.
- S2/S3 must add source foreign/tensor types, scope guards, effect/capability
  enforcement, compiler lowering and real `.cg` package examples; recipes are
  in SDK-V2.md and INFERENCE-V4.md. Neither Rust tests nor empty package source
  modules count as source-level integration acceptance.
- Native SDK hashes its facade, not its complete system dependency closure;
  reviewed native dependencies are required. Native faults, malicious C ABI
  violations and concurrent hostile file mutation are outside the safe wrapper
  contract. Pin drift can quarantine/leak an unreleasable resource instead of
  calling unverifiable code. LibTorch retains its stronger complete closure
  checks. No unrestricted package/hermetic runtime claim follows.
- No zlib/fmt/regex/other ecosystem package or ten-ecosystem source route was
  accepted here. Those require actual pinned adapters after SDK review.
- Quiet-host throughput/p95 and Python/native baseline comparisons remain unrun;
  no timings here are performance evidence. Existing matrix_runtime.py can
  support its reviewed v3 matrix workload after preparing a separate v3 package;
  the v4 full pipeline benchmark/charging protocol is a later S3/S4 acceptance,
  not a renamed matrix benchmark. No paid studies or policy changes occurred.
- Serena initialization created an untracked `.serena/` tool directory outside
  owned code; it is excluded from the commit. All committed paths are owned.
