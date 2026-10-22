# cgx2-libtorch

This crate is a direct native **CPU, one-dimensional `float64`** adapter. It opens a reviewed C++ facade compiled with `torch/torch.h` against an exact LibTorch `Torch_VERSION`. The facade exports the C signatures in `facade/facade.h` under ABI version 1. This is a version-pinned CGx2 C boundary, **not** LibTorch's stable C ABI. `Torch::open(path, exact_version)` checks identity `cgx2.libtorch.cpu`, exact version and ABI before resolving tensor exports; the caller must supply a reviewed, hash-pinned facade artifact. A missing export rejects opening. No Python bridge or fallback is used.

`from_f64` copies `length * 8` input bytes into LibTorch-owned tensor storage. `add` requires equal lengths and creates a new LibTorch tensor; `shape` reports `[length]`. `to_vec_bounded(maximum)` reads the reported shape, requires it to match the length known at handle creation, and rejects values beyond the caller's maximum and `MAX_COPY_ELEMENTS` (1,048,576 elements, 8 MiB) before allocating the copy buffer. The existing `to_vec()` uses that fixed 8 MiB bound. The facade checks CPU, `float64` and one dimension on tensor reads, exact output capacity before copy, and converts C++ exceptions into a status and thread-local message. On failed create/add, output starts null; the Rust adapter releases a nonnull output only if it is not an already owned pointer and the pinned loader remains verified. An unverifiable post-call state can leave a foreign handle unreleased. A successful status with null handle is rejected. Each Rust `Tensor` belongs to one `Torch` adapter instance, holds the facade loaded through its lifetime, and releases its foreign handle once by explicit `release` or `Drop`. Cross-adapter add and double release are rejected. LibTorch owns tensor and device allocations; CGx2 owns wrapper handles and copied host buffers. Foreign allocations are outside CGx2 GC region budgets and reports; CGx2 `no_gc` does not control LibTorch memory.

For a real artifact, configure and build the facade with a compatible LibTorch installation:

```sh
cmake -S crates/cgx2-libtorch/facade -B /tmp/cgx2-libtorch-build \
  -DCMAKE_PREFIX_PATH=/absolute/path/to/libtorch \
  -DCGX2_EXPECT_TORCH_VERSION=EXACT_VERSION
cmake --build /tmp/cgx2-libtorch-build
sha256sum /tmp/cgx2-libtorch-build/libcgx2_libtorch_facade.so
```

Pin that exact `.so` hash, target OS/arch, facade ABI and nine exports in a `cgx2-bind` lock. Preflight only inspects bytes and exported names; it cannot prove that a foreign library actually implements their C types, version assertion or lifetime contract. `Torch::open` is unsafe for this reason. The C++ build rejects a mismatched `Torch_VERSION`, and runtime checks metadata again.

For a fully pinned Linux route, use a `cgx2-bind` schema-2 package and its `cgx2-libtorch-closure.lock.json` sidecar. `Torch::open_pinned(package_root, "torch_cpu")` runs package preflight, rechecks the complete closure, inspects already mapped modules and loader environment before `dlopen`, opens the facade, then checks `/proc/self/maps` mapped paths and inodes before returning. It retains the verified closure and rechecks it before each tensor foreign call. A schema-1 package and the existing unsafe `Torch::open` remain available for compatibility but do not establish closure pinning. Static preflight checks the complete ELF `DT_NEEDED` graph and permits only `$ORIGIN` based `DT_RUNPATH`, rejecting legacy `DT_RPATH`. A changed file, missing or extra dependency, mapped SONAME or basename substitution, loader search-path candidate, or unrelated loaded `libtorch*`/`libc10*` module rejects pinned open. Unreadable, ambiguous, or oversized Linux maps return `UnverifiedLoader`; a failed later check prevents the foreign call and can leave an existing foreign handle unreleased.

The sidecar has `schema_version: 1` and a nonempty `libraries` array. Each entry has `role` (`facade` exactly once, otherwise `dependency`), ELF logical `name`, canonical absolute `path`, lowercase or uppercase 64-digit `sha256`, host `os` and `arch`, and `soname` (string or `null`, matching ELF `DT_SONAME`). Every `DT_NEEDED` edge from the facade and its dependencies must resolve to exactly one entry; unreachable entries are rejected. The facade entry must match the main binding artifact and SHA-256. Paths may not be symlinks or contain traversal. Limits: 1 MiB lock, 128 libraries, 4096-byte path, 2 GiB per library, 8 GiB total, 1 MiB `readelf` output, 8 MiB `/proc/self/maps`, and 512 other mapped ELF files. `readelf` is needed for nonexecuting ELF inspection. The loader check also rehashes held inodes. These checks identify observable bytes and mapped modules. They cannot prove C ABI signatures, tensor aliasing, handle lifetimes, or behavior. `dlopen` can execute a newly loaded constructor before the post-load check, and a concurrent file or loader change can race either check. A hostile native binary or process requires reviewed binaries and operating-system isolation. A reviewed production LibTorch build and broader compatibility runs remain release gates.

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

That legacy test checks `ldd` dependencies for `libpython`, create/add/shape/copy/release and wrong-version rejection. For the closure-verified route, prepare a schema-2 package with the exact compatible installation and run `CGX2_LIBTORCH_PACKAGE=/absolute/package/root RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-libtorch --offline real_pinned_libtorch_when_installed -- --ignored --nocapture`. The compiled C fixtures test good closure, changed/missing/extra dependencies, wrong facade version, and already-loaded search-path substitution offline. Mocks are **not** real LibTorch conformance evidence. GPU, other dtypes/ranks, sanitizer review, and typed `.cg` tensor/FFI integration remain open.

## Wave 16 local evidence and trust boundary

On 2026-10-22, a facade built with C++20 against the locally installed LibTorch 2.14.0 CPU library passed the ignored `real_pinned_libtorch_when_installed` test in a temporary schema-2 package with 13 SHA-256-pinned ELF closure entries. The test completed pinned create, add, bounded copy, explicit release, double-release rejection, and a `/proc/self/maps` check that no `libpython` was mapped. The 2.13.0 installation on this host could not configure its HIP CMake target and did not supply compatibility evidence. The temporary package is not a reviewed distributable artifact. Reproduce with exact `Torch_VERSION=2.14.0`, C++20, the same facade ABI, a `$ORIGIN`-only RUNPATH, and a freshly reviewed closure lock.

Each successful create/add output must be a distinct owned pointer, even if LibTorch tensor storage is shared. The adapter tracks live pointer addresses and rejects a duplicate without releasing the existing owner. A release error consumes the Rust owner because the foreign side may already have freed it; this can leave a foreign allocation leaked if it did not. `to_vec_bounded(maximum)` limits Rust's host copy allocation to `min(maximum, MAX_COPY_ELEMENTS)` elements, but an ABI-violating facade can still overrun the supplied buffer or falsify length. LibTorch host/device allocations remain foreign and outside CGx2 collection accounting and `no_gc` guarantees.

The pinned route checks ELF hashes, `DT_NEEDED`, loader environment, and mapped path/device/inode identity before and after loading and before/after tensor calls. Executable mappings with arbitrary names are checked for deleted or replaced path identity before their SONAME is trusted. These checks have time-of-check/use windows around `dlopen` and foreign calls, and do not sandbox malicious native code or prove the facade's C signatures and allocation behavior. An unverifiable closure blocks further foreign calls, including release, so an existing handle may leak.
