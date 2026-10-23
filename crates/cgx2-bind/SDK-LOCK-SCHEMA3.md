# Additive SDK lock schema 3

Historical lock schemas 1/2 and tensor APIs v1-v3 retain their signatures and
meaning. `BindingLock` schema 3 adds two explicit layouts while retaining all
existing strict fields and conservative checks. New layouts reject in older
schemas. The package manifest stays `cgx2.toml` schema 2, with `binding_lock`.
No required fields or enum variants were added to old public representations.

Native profile: route `native-c`, abi `c`, calling convention `c`, layout
`native-sdk-v2`. `sdk_v2::native_facade_symbols()` supplies the exact nine
declarations (physical C signatures, ownership, release, status, effects and
capabilities). `cgx2_library_identity/version/abi_version` retain the v1 metadata
ABI; `cgx2_sdk_api_version()` must return 2 before operational calls.
`cgx2-ffi-native/sdk_v2.h` is the normative native layout/lifetime specification.
The lock names the exact package/library/version, host OS/arch, package-relative
artifact and SHA-256. Preflight validates confinement, hash and all exports with
bounded inspection, without loading code. Missing/changed signatures reject.
Extra DSO exports do not become allowed invocations.

Tensor profile: route `libtorch`, abi `cgx2-libtorch-facade-v1`, layout
`tensor-f32-cpu-inference-v4`. `sdk_v2::libtorch_inference_facade_symbols()` is
the exact v1+v2+v3+v4 set. v4 adds version, exact-shape f32 addition and ReLU;
the existing `facade.h` signatures stay fixed. Schema 3 requires the same
complete schema-1 ELF closure lock as schema 2, with canonical paths, hashes,
SONAMEs and loaded inode checks. `Torch::open_pinned` requires schema 2/3;
v4 enablement verifies version 4, while facade ABI remains 1. The preparation
script's `--inference` writes schema 3 and the v4 layout deterministically.

Native SDK pins the facade artifact and retains its inode via `/proc/self/fd`;
each operation checks hash and path inode again. It does **not** pin the full
native DSO dependency closure. Reviewed system C/C++ runtimes are required on
the host; this profile is not a hermetic arbitrary-package environment. The
LibTorch route does pin/check its complete declared closure. Metadata/hash/name
checks cannot prove C signatures, exception containment or pointer behavior;
both loaders remain unsafe review boundaries. Constructors execute at dlopen,
and concurrent hostile artifact/map mutation remains outside this contract.

Offline native fixture example (from repository root; normal private build
environment from worker-rules applies):

```sh
c++ -std=c++17 -shared -fPIC -Wall -Wextra -Werror \
  crates/cgx2-ffi-native/tests/fixtures/sdk.cpp -o "$CARGO_TARGET_DIR/libsdk.so"
cargo run --offline -p cgx2-bind --example package_native_sdk -- \
  "$CARGO_TARGET_DIR/libsdk.so" "$CARGO_TARGET_DIR/sdk-package" sdk_fixture 1.0.0
cargo run --offline -p cgx2-bind --bin cgx2-bind -- check "$CARGO_TARGET_DIR/sdk-package"
cargo run --offline -p cgx2-ffi-native --example sdk_roundtrip -- "$CARGO_TARGET_DIR/sdk-package"
```

This is a C++ test facade with variable output and deliberate error cases,
not evidence for zlib/fmt/regex or other external ecosystem packages.
