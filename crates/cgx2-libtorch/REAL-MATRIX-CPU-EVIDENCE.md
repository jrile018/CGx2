# Wave 19 local direct LibTorch matrix CPU run (2026-10-23)

This is one host-specific copied package, not a portable LibTorch binary.
The installed Torch 2.11.0 CPU headers and ELF libraries came from
`/home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch`.
The facade was built C++20 on Linux x86_64 with
`/usr/bin/x86_64-linux-gnu-g++-15` (Ubuntu 15.2.0),
`_GLIBCXX_USE_CXX11_ABI=1` compiler default, and empty `TORCH_CXX_FLAGS`.
Python 3 prepared the package offline; the Rust adapter calls LibTorch C++
directly and the runtime test checks that `/proc/self/maps` has no `libpython`.

From the repository root:

```sh
TORCH_ROOT=/home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch
BUILD=/tmp/cgx2-wave19-libtorch-build
PACKAGE=/tmp/cgx2-wave19-libtorch-package
COMPILER=/usr/bin/x86_64-linux-gnu-g++-15
cmake -S crates/cgx2-libtorch/facade -B "$BUILD" \
  -DCMAKE_PREFIX_PATH="$TORCH_ROOT" -DCMAKE_CXX_COMPILER="$COMPILER" \
  -DCGX2_EXPECT_TORCH_VERSION=2.11.0 -DCMAKE_BUILD_TYPE=Release
cmake --build "$BUILD" --config Release -j2
python3 crates/cgx2-libtorch/prepare_pinned.py \
  --facade "$BUILD/libcgx2_libtorch_facade.so" \
  --libtorch-root "$TORCH_ROOT" --build-dir "$BUILD" \
  --package "$PACKAGE" --version 2.11.0 --compiler "$COMPILER" \
  --cxx11-abi 1 --torch-cxx-flags="$(cat "$BUILD/cgx2-torch-cxx-flags.txt")" \
  --build-command="cmake --build $BUILD --config Release -j2" \
  --lib-dir /usr/lib/x86_64-linux-gnu \
  --select-dependency="libgomp.so.1=$TORCH_ROOT/lib/libgomp.so.1" --matrix
RUSTUP_TOOLCHAIN=1.98.1 cargo run -p cgx2-bind --offline --bin cgx2-bind -- check "$PACKAGE"
LD_LIBRARY_PATH="$PACKAGE" CGX2_LIBTORCH_PACKAGE="$PACKAGE" \
  RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-libtorch --offline --test closure \
  real_pinned_matrix_when_installed -- --ignored --nocapture
LD_LIBRARY_PATH="$PACKAGE" CGX2_LIBTORCH_PACKAGE="$PACKAGE" \
  RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-libtorch --offline --test closure \
  real_pinned_libtorch_when_installed -- --ignored --nocapture
find "$PACKAGE" -type f -print0 | sort -z | xargs -0 sha256sum
```

Preflight returned `status ok`, `bindings_checked 1`. The ignored real matrix
test passed (`1 passed`, `0 failed`, 200.01 seconds). It checks
`[2,3] @ [3,2] = [2,2]`, with row-major output `[58,64,139,154]` calculated
as ordinary dot products, exact-capacity copy, explicit release and
double-release rejection, and no mapped `libpython`.
The retained v1 vector test also passed on this package (`1 passed`,
`0 failed`, 160.90 seconds), covering vector create/add/shape/copy/release.

SHA-256 inventory of the copied package (local paths and lock entries are not
committed):

| File | SHA-256 |
| --- | --- |
| `cgx2-bind.lock.json` | `d8319e9ef8d0d8cac22c8dcde36093186955e1721a67e164ae526eec93ac5396` |
| `cgx2-libtorch-closure.lock.json` | `69c652626d345d9e150ef3a894e3badcc9e223b087c40f742b90cc8a847e29ff` |
| `cgx2-libtorch-evidence.json` | `8dbf50279f87355bc25cba1a542aafad2293a58f6aa3b89e9b268032da38759d` |
| `cgx2.toml` | `be4382c0eb86acdc941bc30754e6bbeb08f2dc88c1215f793aa8bd7131e5ce5d` |
| `libc10.so` | `e8ad028c6a3b3f787b619f35d3c772485476c029c2e9faca659a56212ae7c00a` |
| `libcgx2_libtorch_facade.so` | `8af1cc79644d383872782675f84b6f9a819b8b34ab066e0ba2c4bfe31e979b27` |
| `libgomp.so.1` | `78511033caddec6ccae8f4d62b94d56135f377c4cee33120e6d7df2ef499f69f` |
| `libtorch.so` | `59a95807d56ca89993351a173839f160ca9c2c6ec6f6b89a5e691d8f1392d3ff` |
| `libtorch_cpu.so` | `ad0bcc0225bf260968e0464982b78c2f25358400769720ce63c209ffc9929313` |
| `src/main.cg` | `614cb0146fc3b8416dae6139a34c0c78a0e6b11041bc5e3779c249db24813a8c` |

The schema-2 closure still pins host ELF dependencies by canonical path,
inode and hash. This run does not prove the C++ ABI for arbitrary libraries,
eliminate loader time-of-check/use races, cover GPU or other dtypes, or add a
general `.cg` matrix value.
