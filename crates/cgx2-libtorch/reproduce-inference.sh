#!/bin/sh
# Run from repository root, using an already installed matching LibTorch.
set -eu
: "${NATIVE_TORCH_ROOT:?set to canonical installed LibTorch root}"
: "${NATIVE_TORCH_VERSION:?set to exact numeric LibTorch version}"
: "${NATIVE_COMPILER:?set to canonical C++ compiler path}"
: "${NATIVE_CXX11_ABI:?set to matching 0 or 1}"
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-native" TMPDIR="$PWD/target/tmp-native"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
native_build="$PWD/target/native-facade-build"
native_package="$PWD/target/native-real-package"
cmake -S crates/cgx2-libtorch/facade -B "$native_build" \
  -DCMAKE_PREFIX_PATH="$NATIVE_TORCH_ROOT" -DCMAKE_CXX_COMPILER="$NATIVE_COMPILER" \
  -DCGX2_EXPECT_TORCH_VERSION="$NATIVE_TORCH_VERSION" -DCMAKE_BUILD_TYPE=Release
cmake --build "$native_build" --config Release -j2
python3 crates/cgx2-libtorch/prepare_pinned.py \
  --facade "$native_build/libcgx2_libtorch_facade.so" \
  --libtorch-root "$NATIVE_TORCH_ROOT" --build-dir "$native_build" \
  --package "$native_package" --version "$NATIVE_TORCH_VERSION" \
  --compiler "$NATIVE_COMPILER" --cxx11-abi "$NATIVE_CXX11_ABI" \
  --torch-cxx-flags="$(cat "$native_build/cgx2-torch-cxx-flags.txt")" \
  --build-command="cmake --build $native_build --config Release -j2" --inference "$@"
cargo run --offline -p cgx2-bind --bin cgx2-bind -- check "$native_package"
LD_LIBRARY_PATH="$native_package" CGX2_LIBTORCH_REAL_PACKAGE="$native_package" \
  cargo test --release --offline --all-features -p cgx2-libtorch --test inference \
  real_pinned_two_layer_inference_no_python -- --ignored --exact --nocapture
LD_LIBRARY_PATH="$native_package" CGX2_LIBTORCH_REAL_PACKAGE="$native_package" \
  cargo test --release --offline --all-features -p cgx2-libtorch --test matrix_f32 \
  real_pinned_float32_reference_and_cycles -- --ignored --exact --nocapture
LD_LIBRARY_PATH="$native_package" cargo run --release --offline -p cgx2-libtorch --example infer_cpu -- "$native_package"
