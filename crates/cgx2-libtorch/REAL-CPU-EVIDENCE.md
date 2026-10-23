# Wave 18 local direct LibTorch CPU run (2026-10-23)

This is one reproducible local run, not a portable binary release. The source
was an already installed Torch 2.11.0 CPU distribution at
`/home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch`.
The facade used C++20, `/usr/bin/x86_64-linux-gnu-g++-15` (Ubuntu 15.2.0),
Linux x86_64, and `_GLIBCXX_USE_CXX11_ABI=1` from the compiler default;
`TORCH_CXX_FLAGS` was empty. The C++ facade stores error text in a fixed
4096 byte thread local buffer. The Python package supplied C++ headers and ELF
libraries. CGx2 called the copied C++ facade directly; the CPU test asserted
that `/proc/self/maps` contained no `libpython` after tensor operations.

From the repository root, the exact build and preparation were:

```sh
LIBTORCH_ROOT=/home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch
BUILD_DIR=/tmp/cgx2-wave18-libtorch-build
PACKAGE=/tmp/cgx2-wave18-libtorch-package
COMPILER=/usr/bin/x86_64-linux-gnu-g++-15
cmake -S crates/cgx2-libtorch/facade -B "$BUILD_DIR" \
  -DCMAKE_PREFIX_PATH="$LIBTORCH_ROOT" -DCMAKE_CXX_COMPILER="$COMPILER" \
  -DCGX2_EXPECT_TORCH_VERSION=2.11.0 -DCMAKE_BUILD_TYPE=Release
cmake --build "$BUILD_DIR" --config Release -j2
python3 crates/cgx2-libtorch/prepare_pinned.py \
  --facade "$BUILD_DIR/libcgx2_libtorch_facade.so" \
  --libtorch-root "$LIBTORCH_ROOT" --build-dir "$BUILD_DIR" \
  --package "$PACKAGE" --version 2.11.0 --compiler "$COMPILER" \
  --cxx11-abi 1 --torch-cxx-flags="$(cat "$BUILD_DIR/cgx2-torch-cxx-flags.txt")" \
  --build-command='cmake --build /tmp/cgx2-wave18-libtorch-build --config Release -j2' \
  --lib-dir /usr/lib/x86_64-linux-gnu \
  --select-dependency=libgomp.so.1="$LIBTORCH_ROOT/lib/libgomp.so.1"
RUSTUP_TOOLCHAIN=1.98.1 cargo run -p cgx2-bind --offline --bin cgx2-bind -- check "$PACKAGE"
LD_LIBRARY_PATH="$PACKAGE" CGX2_LIBTORCH_PACKAGE="$PACKAGE" \
  RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-libtorch --offline --test closure \
  real_pinned_libtorch_when_installed -- --ignored --nocapture
```

Preflight returned `status ok`, `bindings_checked 1`. The ignored real test
passed in 134.18 seconds with the fixed error buffer: create two CPU float64 tensors, add, shape `[3]`,
bounded copy `[5.0, 7.0, 9.0]`, explicit release of all three handles,
double-release rejection, and no mapped `libpython`. Preparation run twice
with identical inputs produced byte-identical binding, closure, and evidence
JSON files. The compiled C fixtures in the offline suite remain **mock**
evidence.

SHA-256 inventory of all files in the local copied package:

| File | SHA-256 |
| --- | --- |
| `libc10.so` | `e8ad028c6a3b3f787b619f35d3c772485476c029c2e9faca659a56212ae7c00a` |
| `libcgx2_libtorch_facade.so` | `41733167733597f23147518596054b8f9cd7838999088d1a17c48a6ceca3d2f6` |
| `libgomp.so.1` | `78511033caddec6ccae8f4d62b94d56135f377c4cee33120e6d7df2ef499f69f` |
| `libtorch.so` | `59a95807d56ca89993351a173839f160ca9c2c6ec6f6b89a5e691d8f1392d3ff` |
| `libtorch_cpu.so` | `ad0bcc0225bf260968e0464982b78c2f25358400769720ce63c209ffc9929313` |
| `cgx2.toml` | `be4382c0eb86acdc941bc30754e6bbeb08f2dc88c1215f793aa8bd7131e5ce5d` |
| `cgx2-bind.lock.json` | `de8258d37ae70a564832e23e5d6d938858ecf6c0c1c1c92739b47217a9aa1f74` |
| `cgx2-libtorch-closure.lock.json` | `61679304e3fcf7fcbbdec7d2c8f1a292c87fd7fc625d78a6af6f09a56a1267da` |
| `cgx2-libtorch-evidence.json` | `03eea7a665a12bfd8e7d6cf4896c0da87aabb11f16878b878f4a1821d5541d24` |
| `src/main.cg` | `614cb0146fc3b8416dae6139a34c0c78a0e6b11041bc5e3779c249db24813a8c` |

The closure lock also pins the actual `/usr/lib/x86_64-linux-gnu` host ELF
dependencies by SHA-256, path, and SONAME. Those are host specific and were
not copied into the package. The local package and its absolute locks are in
`/tmp`, not committed. Regenerate them for another host. Static `DT_NEEDED`
closure checks cannot rule out a later `dlopen`, and an ABI-violating binary
can still forge handles or invalid pointers. Foreign allocations and device
storage remain outside CGx2 memory accounting. `.cg` source tensor calls are
not connected to this adapter.
