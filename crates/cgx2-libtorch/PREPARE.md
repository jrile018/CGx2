# Reproducible local LibTorch package
For the additive matrix route, compile the current facade and append
`--matrix` to `prepare_pinned.py`. This pins layout
`tensor-f64-cpu-matrix-v2`, its five additional exports, and all v1 exports.
Without `--matrix`, preparation keeps the v1 vector lock. The matrix API
version is 2 while the facade C ABI remains 1. Run the real matrix test with
`CGX2_LIBTORCH_PACKAGE=PACKAGE` and
`cargo test -p cgx2-libtorch --offline --test closure real_pinned_matrix_when_installed -- --ignored --nocapture`
under `LD_LIBRARY_PATH=PACKAGE`. The test checks non-square multiplication,
exact-capacity copy, release, and no mapped `libpython`.

`prepare_pinned.py` takes an **already built**, exact LibTorch installation and
facade; it never downloads anything. Preparation uses Python 3, while the
CGx2 adapter and C++ facade use no Python interpreter or `libpython`. The facade CMake target uses `$ORIGIN`
RUNPATH and writes its configured `TORCH_CXX_FLAGS` into the build directory.
Use canonical absolute paths for all arguments. List every dependency search
directory explicitly with `--lib-dir`; `LIBTORCH_ROOT/lib` is included
automatically. If a needed filename resolves to two different files, preparation
fails. Rerunning the same command with unchanged inputs makes byte-identical
locks and evidence.

```sh
LIBTORCH_ROOT=/absolute/path/to/exact/libtorch
BUILD_DIR=/absolute/path/to/facade-build
PACKAGE=/absolute/path/to/local-package
VERSION=EXACT_MAJOR.MINOR.PATCH
COMPILER=/absolute/canonical/path/to/c++
cmake -S crates/cgx2-libtorch/facade -B "$BUILD_DIR" \
  -DCMAKE_PREFIX_PATH="$LIBTORCH_ROOT" \
  -DCMAKE_CXX_COMPILER="$COMPILER" \
  -DCGX2_EXPECT_TORCH_VERSION="$VERSION"
cmake --build "$BUILD_DIR" --config Release
python3 crates/cgx2-libtorch/prepare_pinned.py \
  --facade "$BUILD_DIR/libcgx2_libtorch_facade.so" \
  --libtorch-root "$LIBTORCH_ROOT" --build-dir "$BUILD_DIR" \
  --package "$PACKAGE" --version "$VERSION" --compiler "$COMPILER" \
  --cxx11-abi 1 \
  --torch-cxx-flags="$(cat "$BUILD_DIR/cgx2-torch-cxx-flags.txt")" \
  --build-command="cmake --build $BUILD_DIR --config Release" \
  --lib-dir /absolute/canonical/system/elf-directory
RUSTUP_TOOLCHAIN=1.98.1 cargo run -p cgx2-bind --offline --bin cgx2-bind -- check "$PACKAGE"
LD_LIBRARY_PATH="$PACKAGE${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  CGX2_LIBTORCH_PACKAGE="$PACKAGE" RUSTUP_TOOLCHAIN=1.98.1 \
  cargo test -p cgx2-libtorch --offline real_pinned_libtorch_when_installed \
  -- --ignored --nocapture
find "$PACKAGE" -type f -print0 | sort -z | xargs -0 sha256sum
```

Set `--cxx11-abi` to the value in the configured flags (0 or 1), or to the
compiler's default when `TORCH_CXX_FLAGS` contains no ABI definition. Include
additional exact `--lib-dir` arguments as needed. The script checks CMake's
version, compiler, prefix, and flags evidence; hashes every file while checking
size and inode stability; records canonical paths, SONAMEs, DT_NEEDED,
RUNPATH, sizes, hashes, platform, compiler, ABI, configured flags, and the
declared build and preparation commands in `cgx2-libtorch-evidence.json`.
The destination contains regular, nonsymlink copies of the facade and every
non-system dependency, named by ELF `DT_NEEDED`. The closure lock points to
the copied paths. The ELF interpreter and host libraries under `/usr/lib` or
`/lib` remain pinned at their canonical host paths because they may already
be mapped before the facade loads. Evidence records `source_path`, deployed
`path`, and `packaged` for each library. Moving the package or changing a
host library requires regenerating and reviewing the absolute lock paths.
Destination library and metadata symlinks are rejected.
If two directories contain the same DT_NEEDED name, the script rejects the
ambiguous closure. Resolve an intentional duplicate with
`--select-dependency=NAME=/canonical/path/to/chosen/library`; the selected
path must be one of the discovered candidates. The actual loader search path
must select the same pinned library for `Torch::open_pinned` to succeed.
Its binding and closure locks are checked by schema-2 preflight. The ignored
real test then checks `Torch::open_pinned`, create/add/shape, bounded copy,
explicit release, and absence of mapped `libpython`. Record the exact command,
package hashes, and test outcome with any real run.

`cargo test -p cgx2-bind -p cgx2-libtorch --offline` runs **mock**
negative fixtures: wrong facade identity/ABI/version, missing export,
ambiguous dependency candidates and mapped SONAME, changed dependency
inode/hash, bad closure metadata, and returned handle alias/error behavior.
These tests do not establish real LibTorch behavior. Static DT_NEEDED closure
cannot observe later runtime `dlopen`; constructor code executes during
`dlopen` before the post-load map check. Files or loader state can change
between checks. A malicious facade can violate the declared C ABI or forge
handles; a rejected nonnull output can be released only when it is distinct
from live handles and the closure remains verified. Foreign allocations and
device storage remain outside CGx2 memory accounting; host copies remain
bounded by `MAX_COPY_ELEMENTS` and the caller's maximum. The C++ facade uses a
fixed 4096 byte thread local error buffer, and Rust scans at most
`MAX_ERROR_BYTES` (4096) before converting it to a string;
an ABI-violating pointer can still cause undefined behavior.
