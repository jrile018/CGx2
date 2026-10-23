#!/bin/sh
# Run from repository root. Does not install, download or change host policy.
set -eu
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-native" TMPDIR="$PWD/target/tmp-native"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test --offline --all-features -p cgx2-ffi-core -p cgx2-bind -p cgx2-ffi-native -p cgx2-libtorch
cargo clippy --offline --all-targets --all-features -p cgx2-ffi-core -p cgx2-bind -p cgx2-ffi-native -p cgx2-libtorch -- -D warnings
cargo fmt --check -p cgx2-ffi-core -p cgx2-bind -p cgx2-ffi-native -p cgx2-libtorch
git diff --check
c++ -std=c++17 -shared -fPIC -Wall -Wextra -Werror crates/cgx2-ffi-native/tests/fixtures/sdk.cpp -o "$CARGO_TARGET_DIR/libsdk.so"
cargo run --offline -p cgx2-bind --example package_native_sdk -- "$CARGO_TARGET_DIR/libsdk.so" "$CARGO_TARGET_DIR/sdk-package" sdk_fixture 1.0.0
cargo run --offline -p cgx2-bind --bin cgx2-bind -- check "$CARGO_TARGET_DIR/sdk-package"
cargo run --offline -p cgx2-ffi-native --example sdk_roundtrip -- "$CARGO_TARGET_DIR/sdk-package"
if [ "${1:-}" = --leaks ]; then
  c++ -std=c++17 -g -fsanitize=address,undefined -fno-omit-frame-pointer \
    crates/cgx2-ffi-native/tests/fixtures/sdk.cpp crates/cgx2-ffi-native/tests/fixtures/sdk_asan.cpp \
    -o "$CARGO_TARGET_DIR/sdk-asan"
  ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 "$CARGO_TARGET_DIR/sdk-asan"
fi
