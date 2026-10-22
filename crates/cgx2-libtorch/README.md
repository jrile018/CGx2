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

For a fully pinned Linux route, use a `cgx2-bind` schema-2 package and its `cgx2-libtorch-closure.lock.json` sidecar. `Torch::open_pinned(package_root, "torch_cpu")` runs package preflight, rechecks the complete closure, opens the facade, then checks `/proc/self/maps` mapped paths and inodes before returning a tensor-capable adapter. A schema-1 package and the existing unsafe `Torch::open` remain available for compatibility but do not establish closure pinning. A schema-2 preflight alone checks files and ELF `DT_NEEDED` edges without loading code; the post-open check establishes which inodes the Linux loader actually mapped. A changed file, missing or extra closure entry, substituted search-path dependency, or unrelated loaded `libtorch*`/`libc10*` module rejects pinned open. If Linux maps cannot be read, `open_pinned` returns a distinct `UnverifiedLoader` error.

The sidecar has `schema_version: 1` and a nonempty `libraries` array. Each entry has `role` (`facade` exactly once, otherwise `dependency`), ELF logical `name`, canonical absolute `path`, lowercase or uppercase 64-digit `sha256`, host `os` and `arch`, and `soname` (string or `null`, matching ELF `DT_SONAME`). Every `DT_NEEDED` edge from the facade and its dependencies must resolve to exactly one entry; unreachable entries are rejected. The facade entry must match the main binding artifact and SHA-256. Paths may not be symlinks or contain traversal. Limits: 1 MiB lock, 128 libraries, 4096-byte path, 2 GiB per library, 8 GiB total. `readelf` is needed for nonexecuting ELF inspection. The loader check also rehashes held inodes after opening. These checks identify bytes and mapped modules; they do not prove C ABI signatures, tensor aliasing, handle lifetimes, or behavior, and a hostile process that can mutate mapped files concurrently needs separate operating-system isolation. A compatible real LibTorch run remains a release gate.

To prepare a real package, recursively inspect the facade and every discovered dependency with `readelf -d /absolute/canonical/library.so`, recording each `NEEDED` and `SONAME`. Resolve each needed name to the exact library intended for deployment, record `realpath` and `sha256sum` of every file, and include all reachable libraries, including system libraries. Put the facade's relative artifact name and matching SHA-256 in `cgx2-bind.lock.json` with `schema_version: 2`; put all absolute entries in the sidecar. For example, an entry is `{"role":"dependency","name":"libtorch_cpu.so","path":"/absolute/libtorch/lib/libtorch_cpu.so","sha256":"<64 hex digits>","os":"linux","arch":"x86_64","soname":"libtorch_cpu.so"}`. Run `cgx2-bind check PACKAGE` before `Torch::open_pinned`. Copying a package or changing the host library set requires regenerating and reviewing the closure lock.

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

That legacy test checks `ldd` dependencies for `libpython`, create/add/shape/copy/release and wrong-version rejection. For the closure-verified route, prepare a schema-2 package with the exact compatible installation and run `CGX2_LIBTORCH_PACKAGE=/absolute/package/root RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-libtorch --offline real_pinned_libtorch_when_installed -- --ignored --nocapture`. The compiled C fixtures test good closure, changed/missing/extra dependencies, wrong facade version, and already-loaded search-path substitution offline. Mocks are **not** real LibTorch conformance evidence. GPU, other dtypes/ranks, a compatible real artifact on this host, sanitizer review, and typed `.cg` tensor/FFI integration remain open.
