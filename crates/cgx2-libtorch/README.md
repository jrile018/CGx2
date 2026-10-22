# cgx2-libtorch

Wave 10 direct native **CPU** LibTorch prototype. The public Rust route opens a C++ facade built against an **exact LibTorch version**. It never imports Python, links a Python bridge, or falls back to CPython. The typed profile is one-dimensional CPU `float64`: create from a Rust slice, add equal-shaped tensors, inspect shape, copy values back, and release. Tensor storage belongs to LibTorch; input and output slices are copied. Handles remain tied to the adapter that created them and are released once. LibTorch errors become `Error::Foreign` with status and message.

The official [LibTorch stable ABI notes](https://docs.pytorch.org/docs/stable/notes/libtorch_stable_abi.html) describe a limited extension API, so this prototype uses a version-pinned C++ facade over `torch/torch.h`. `facade/CMakeLists.txt` rejects a mismatched `Torch_VERSION`; the runtime declaration also checks the exact version string and ABI 1. `facade.h` defines the C ABI. The C++ implementation catches all exceptions before returning to Rust. The facade exports only the CPU profile and does not claim a GPU or general tensor binding.

To run the real CPU conformance test with a compatible LibTorch distribution:

```sh
cmake -S crates/cgx2-libtorch/facade -B /tmp/cgx2-libtorch-build \
  -DCMAKE_PREFIX_PATH=/absolute/path/to/libtorch \
  -DCGX2_EXPECT_TORCH_VERSION=EXACT_VERSION
cmake --build /tmp/cgx2-libtorch-build
CGX2_LIBTORCH_FACADE=/tmp/cgx2-libtorch-build/libcgx2_libtorch_facade.so \
CGX2_LIBTORCH_VERSION=EXACT_VERSION \
RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-libtorch --offline -- --ignored --nocapture
```

The host used for Wave 10 has Rust 1.98.1, GCC 15.2.0, and CMake 4.2.3, but no compatible LibTorch distribution in the checked system, `/opt`, or project paths. The real CPU test is therefore **unavailable** and is explicitly marked ignored until run with the commands above. The static route test checks that the manifest/build/facade sources do not reference Python libraries and C-compiles the ABI header. With a real facade, the CPU test additionally checks `ldd` for a direct/transitive `libpython` dependency, exercises create/add/shape/copy/release, and rejects an incorrect version. The C++ facade could not be compiled or runtime-checked on this host. GPU conformance, ABI stress/sanitizers, and `.cg` source calls remain open gates.
