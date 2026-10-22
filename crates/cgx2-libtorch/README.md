# cgx2-libtorch

This crate is a direct native **CPU, one-dimensional `float64`** adapter. It opens a reviewed C++ facade compiled with `torch/torch.h` against an exact LibTorch `Torch_VERSION`. The facade exports the C signatures in `facade/facade.h` under ABI version 1. This is a version-pinned CGx2 C boundary, **not** LibTorch's stable C ABI. `Torch::open(path, exact_version)` checks identity `cgx2.libtorch.cpu`, exact version and ABI before resolving tensor exports; the caller must supply a reviewed, hash-pinned facade artifact. A missing export rejects opening. No Python bridge or fallback is used.

`from_f64` copies `length * 8` input bytes into LibTorch-owned tensor storage. `add` requires equal lengths and creates a new LibTorch tensor; `shape` reports `[length]`; `to_vec` copies `length * 8` bytes into a zero-initialized Rust buffer. The facade checks CPU, `float64` and one dimension on tensor reads, exact output capacity before copy, and converts C++ exceptions into a status and thread-local message. On failed create/add, output starts null and the Rust adapter releases any nonnull handle before returning an error. A successful status with null handle is rejected. Each Rust `Tensor` belongs to one `Torch` adapter instance, holds the facade loaded through its lifetime, and releases its foreign handle once by explicit `release` or `Drop`. Cross-adapter add and double release are rejected. LibTorch owns tensor and device allocations; CGx2 owns wrapper handles and copied host buffers. CGx2 `no_gc` does not control LibTorch memory.

For a real artifact, configure and build the facade with a compatible LibTorch installation:

```sh
cmake -S crates/cgx2-libtorch/facade -B /tmp/cgx2-libtorch-build \
  -DCMAKE_PREFIX_PATH=/absolute/path/to/libtorch \
  -DCGX2_EXPECT_TORCH_VERSION=EXACT_VERSION
cmake --build /tmp/cgx2-libtorch-build
sha256sum /tmp/cgx2-libtorch-build/libcgx2_libtorch_facade.so
```

Pin that exact `.so` hash, target OS/arch, facade ABI and nine exports in a `cgx2-bind` lock. Preflight only inspects bytes and exported names; it cannot prove that a foreign library actually implements their C types, version assertion or lifetime contract. `Torch::open` is unsafe for this reason. The C++ build rejects a mismatched `Torch_VERSION`, and runtime checks metadata again.

The lock pins the **facade file only**. The facade links LibTorch dynamically; its transitive `.so` dependencies and loader search paths are not hashed or controlled here. A matching facade hash and compiled version string therefore do not prove that the loaded LibTorch binaries match the reviewed build. Dependency-closure pinning and a compatible real-LibTorch run remain required before this route can be called fully pinned or release ready.

Run mock facade tests and the offline binding tests:

```sh
RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-libtorch -p cgx2-bind --offline
```

The real test is ignored in the offline suite. When the exact compatible artifact exists, run:

```sh
CGX2_LIBTORCH_FACADE=/absolute/path/to/libcgx2_libtorch_facade.so \
CGX2_LIBTORCH_VERSION=EXACT_VERSION \
RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-libtorch --offline \
  real_libtorch_cpu_when_installed -- --ignored --nocapture
```

That test checks transitive `ldd` dependencies for `libpython`, create/add/shape/copy/release and wrong-version rejection. The compiled C mock tests negative metadata, missing export, shape/profile errors, output-copy bounds, foreign failures, cleanup, cross-adapter handles and double release. The mock is **not** a real LibTorch conformance run. GPU, other dtypes/ranks, a compatible real artifact on this host, sanitizer review, and typed `.cg` tensor/FFI integration remain open.
