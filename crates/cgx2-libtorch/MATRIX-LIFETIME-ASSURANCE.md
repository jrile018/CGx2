# Wave 20 matrix lifetime assurance (2026-10-23)

This is evidence for one Linux x86_64 CPU package and the frozen v2 matrix
facade. `tests/matrix_lifetime.rs` keeps the deterministic C mock corpus
separate from its ignored, real LibTorch boundary test.

## Copied package and loader closure

The package at `/tmp/cgx2-wave19-libtorch-package` was prepared from
`/home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch`,
Torch **2.11.0 CPU**. The facade was built with C++20 using
`/usr/bin/x86_64-linux-gnu-g++-15`, `_GLIBCXX_USE_CXX11_ABI=1`, on Linux
**x86_64**. Its copied `libcgx2_libtorch_facade.so` SHA-256 is
`8af1cc79644d383872782675f84b6f9a819b8b34ab066e0ba2c4bfe31e979b27`.
The package has schema-2 lock and evidence files. The closure in the evidence
file, including resolved host libraries, is:

| ELF | Location | SHA-256 |
| --- | --- | --- |
| `ld-linux-x86-64.so.2` | system | `bda779ea9e9ad234f60315477d7cfd0bdde861cd8ddd9df8cdaf62f18c4cec14` |
| `libc.so.6` | system | `85e64f97e348786a8fb4d9f3d52fec289e2fb86bba20f0731dfe61990525e0f7` |
| `libc10.so` | package | `e8ad028c6a3b3f787b619f35d3c772485476c029c2e9faca659a56212ae7c00a` |
| `libcgx2_libtorch_facade.so` | package | `8af1cc79644d383872782675f84b6f9a819b8b34ab066e0ba2c4bfe31e979b27` |
| `libdl.so.2` | system | `59d725ca30596a9863f4fa2552a7125f834842abc86b40890231c129358965d2` |
| `libgcc_s.so.1` | system | `9d339ecb409578d6a5d587e6c537a8f9589b8a13fefba30d167433a4b5758bee` |
| `libgomp.so.1` | package | `78511033caddec6ccae8f4d62b94d56135f377c4cee33120e6d7df2ef499f69f` |
| `libm.so.6` | system | `1c488db6f4fe8cecb72456bc018de000b7a9b539c2994864b0416b5e8371b3d8` |
| `libpthread.so.0` | system | `1327fc40710691dbd94e88b6cf4efce27fefe8fbb93f7d1164a75b4dccaea696` |
| `librt.so.1` | system | `9f3cc59207bcaaac74aaf7aca961f4816f0f4d945e4bd1c3cb98ec516becf161` |
| `libstdc++.so.6` | system | `5bb0d21308f123b6ad46c6f35b42cedfcb8d6d439a53aa3dae04d880aaffdde3` |
| `libtorch.so` | package | `59a95807d56ca89993351a173839f160ca9c2c6ec6f6b89a5e691d8f1392d3ff` |
| `libtorch_cpu.so` | package | `ad0bcc0225bf260968e0464982b78c2f25358400769720ce63c209ffc9929313` |

Before the real run, `cargo run --offline -p cgx2-bind --bin cgx2-bind --
check /tmp/cgx2-wave19-libtorch-package` returned `status ok` and
`bindings_checked 1`. `Torch::open_pinned` then checked the complete closure
and mapped modules; it rechecked the closure before each foreign call.
The real test passed with `LD_LIBRARY_PATH` and `CGX2_LIBTORCH_PACKAGE` both
set to this package:

```sh
LD_LIBRARY_PATH=/tmp/cgx2-wave19-libtorch-package \
CGX2_LIBTORCH_PACKAGE=/tmp/cgx2-wave19-libtorch-package \
RUSTUP_TOOLCHAIN=1.98.1 cargo test --offline -p cgx2-libtorch \
  --test matrix_lifetime real_pinned_matrix_element_boundaries \
  -- --ignored --nocapture
```

Result: **1 passed, 0 failed** in 171.08 seconds. It created a
`[1_048_576, 1]` input and distinct `[1, 1]` input, multiplied to the
maximum permitted 1,048,576-element result, checked shape and all copied
values, rejected a short destination, and released handles in a different
order. Zero, overflowing, and over-limit dimensions returned
`InvalidMatrixShape` before foreign calls. `/proc/self/maps` contained no
`libpython` after the calls.

## Mock ABI and dynamic instrumentation

The offline mock corpus cycles through six shapes, including 1 and 8 (the
mock's storage boundary), and alternates release order. It verifies guarded
exact-capacity copies, untouched undersized destinations, no live handles
after each cycle, exact release counts, and `AlreadyReleased` for a second
release. Three fault rounds exercise a partial create, failed copy, and
partial multiply; returned handles are released once. Other assertions pin
`InvalidMatrixShape`, `CapacityMismatch`, `MatrixDimensionMismatch`,
`MatrixApiVersion`, and `MissingSymbol` error classes. The C mock is not
LibTorch conformance evidence.

`cargo test --offline -p cgx2-bind -p cgx2-libtorch` passed using
`CARGO_TARGET_DIR=/tmp/cgx2-wave20-matrix-target`: 25 passed, 4 ignored,
0 failed. `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check` passed.
`CARGO_TARGET_DIR=/tmp/cgx2-wave20-matrix-target RUSTUP_TOOLCHAIN=1.98.1
cargo clippy --offline -p cgx2-bind -p cgx2-libtorch --all-targets --
-D warnings` passed.

GCC AddressSanitizer instrumented `facade/mock.c`, including its C functions
called by the Rust test binary. The reproducible boundary run was:

```sh
CGX2_MATRIX_MOCK_ASAN=1 \
LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libasan.so.8 \
ASAN_OPTIONS=detect_leaks=0:halt_on_error=1 \
/tmp/cgx2-wave20-matrix-target/debug/deps/matrix_lifetime-253b5286ff10d110 \
  mock_matrix --nocapture
```

Result: **2 passed, 0 failed**, with no AddressSanitizer findings. The
compiled C mock uses `-fsanitize=address -fno-omit-frame-pointer -g` when
`CGX2_MATRIX_MOCK_ASAN` is set. LeakSanitizer with `detect_leaks=1` was
attempted but failed in this sandbox (`LeakSanitizer does not work under
ptrace`); `detect_leaks=0` permits the AddressSanitizer memory checks.

The copied real facade has no `__asan_` or `__ubsan_` symbols (`readelf -Ws
libcgx2_libtorch_facade.so | rg '__asan_|__ubsan_'` returned no matches),
and Valgrind is unavailable on this host. Preloading AddressSanitizer cannot
instrument the prebuilt C++ code. The real foreign-memory safety gate remains
open pending an instrumented build or equivalent dynamic tool. These tests
do not establish GPU, other dtype/rank, arbitrary Python package, or broad
language compatibility. No incompatible v2 ABI change was made.
