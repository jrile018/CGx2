# CGx2 interoperability architecture

Status: design proposal, 2026-10-22. No direct LibTorch implementation is claimed here. `docs/language-design.md` now adopts the direct native route; Python-only packages retain an explicit CPython route.

The ten-ecosystem expansion and its separate acceptance criteria are in `docs/interoperability-top10.md`.

## What “direct” means

Bits do not carry enough information to call an arbitrary library. A caller must also agree with the callee on symbol names, calling convention, sizes and alignment, argument and result layout, string encoding, ownership and destruction, errors, thread rules, and required runtime. Without that contract, treating memory as bytes can corrupt memory or call the wrong function.

CGx2 should make these contracts first-class. The programmer uses a typed CGx2 facade; the compiler checks it against a versioned foreign declaration; a small adapter obeys the target's published interface. Raw pointer, cast, and bit-level operations are confined to `unsafe foreign` adapter code. Ordinary `.cg` callers never gain arbitrary pointer access by importing a package.

“Works with any language” means there is an adapter route for libraries that expose an interface CGx2 can target. It does not mean every package can run without its original runtime, that every function is automatically bindable, or that data sharing is always zero-copy.

## Routes, in preferred order

| Target | CGx2 route | Runtime requirement |
| --- | --- | --- |
| Documented native C ABI | Direct FFI; generated checked facade | Native library only |
| C++ or Rust library without a stable C ABI | Small library-specific C ABI facade built with compatible toolchain | Native library plus facade |
| PyTorch tensors and operators | Direct LibTorch stable ABI where available; otherwise version-pinned C++ facade | LibTorch and relevant device libraries; no Python |
| WebAssembly component with declared interface | Component-model adapter after native foundation | WebAssembly runtime |
| Python-only package | Explicit CPython adapter, in process where supportable | Python interpreter and package |
| JVM or .NET-only package | Runtime-specific adapter or declared service boundary | Corresponding runtime |
| Remote or isolated library | Typed process/RPC adapter | Target process and protocol |

The caller selects an explicit route. CGx2 must not silently fall back from a native LibTorch call to Python, and a route that cannot satisfy a declaration fails at build or load time.

## Foreign contract v1

Each binding declares: `schema_version`, package and library identity, exact library version range or hash, target OS/architecture, ABI and calling convention, exported symbol, parameter and result types, layout and encoding, ownership and release function, error convention, thread/callback rules, effects and capabilities. Load-time checks reject mismatched libraries or missing symbols before a user call.

The first portable type set is fixed-width integers and floats, opaque handles, UTF-8 pointer-plus-length strings, and pointer-plus-length byte buffers. A plain `bool`, host-sized integer, native string, arbitrary C++ class, or struct is **not** portable without a declared representation. Structs and callbacks require a separate, checked extension to v1. Foreign exceptions may not unwind through CGx2 frames; adapters translate them to typed `ForeignError` results. `unsafe foreign` is required to author or call unchecked raw-pointer bindings.

Provide separate safe bit-oriented utilities for fixed-width masks, shifts, endian-aware encoding/decoding, and bounds-checked byte slices. They make binary formats convenient without granting access to arbitrary addresses. Reinterpreting bytes as a foreign struct requires a declared layout and validation; unchecked casts remain `unsafe foreign`.

Every borrowed buffer names its owner and valid lifetime. Every owned foreign handle names a destructor/release operation and the runtime that must execute it. Passing CGx2 memory into a foreign call is a copy by default. Zero-copy requires a verified layout, alignment, device, exclusive-versus-shared access, synchronization, and owner-retention contract. A foreign library retains control of its own allocator or collector. CGx2 `no_gc` only constrains CGx2 collection; it cannot promise to suppress a foreign runtime's allocation or GC.

Foreign calls declare `foreign` plus relevant allocation, I/O, callback, blocking, thread, or GPU effects. Calls that may execute arbitrary code cannot be marked pure. The compiler's existing effect/capability checks must apply across imported modules. A callback into CGx2 must define thread attachment, reentrancy, lifetime, and error handling before it is exposed.

## Tensor and columnar exchange

Use direct LibTorch handles for CGx2's primary PyTorch API. Check CPU/GPU availability and tensor dtype, shape, device, aliasing, and mutation. DLPack can be an optional tensor exchange format for *other* supporting libraries, and Arrow C Data can be an optional table exchange format. Neither format supplies arbitrary function calls. Copying remains the safe fallback when a zero-copy lifetime or device contract cannot be established.

## User experience and tooling

An installed binding package contains the foreign declaration, a generated or reviewed `.cg` facade, build/link/load instructions, supported platform/version matrix, and conformance tests. `cgx2 bind check PACKAGE` should report missing symbols, ABI/version mismatches, unsafe layouts, unavailable runtimes, and effect/ownership conflicts before execution. The API index and LLM tooling expose exact signatures, effects, ownership, supported route, examples, and machine-readable diagnostics. Header/interface generation is permitted only for the subset whose semantics can be derived; ambiguous ownership or callbacks require human-authored declarations.

Example, illustrative syntax rather than frozen grammar:

```cg
import torch
let x = torch.tensor([1.0, 2.0], device: cpu)?
let y = x + x
```

This must dispatch to LibTorch without importing Python. A Python-only package may use a separate explicit `python` binding with its own runtime requirement.

## Acceptance gates

1. A `.cg` program calls a small C library and receives correct values and typed errors on supported platforms.
2. Wrong symbol, version, calling convention, layout, or ownership declaration fails before unsafe execution where detectable.
3. Move, clone, borrow, callback, error, and destructor tests cover normal and failure paths; sanitizers or equivalent dynamic checks catch adapter faults.
4. The direct LibTorch example creates/operates on CPU tensors and, when available, GPU tensors; it runs without loading `libpython` and matches a native LibTorch reference.
5. A Python-only fixture works only through the explicitly selected Python route. No native-to-Python fallback occurs implicitly.
6. Compare tiny-call latency, bulk throughput, copy count, peak memory, and correctness against a hand-written native wrapper and the corresponding native library. A preliminary target is no more than 25% extra median latency for a checked scalar call over the hand-written wrapper; calibrate it with repeated measurements and confidence intervals before using it as a release claim.
7. In held-out library-use tasks, measure whether LLMs can select the correct route and ownership rules, complete correct `.cg` integrations, and locate structured API information. Compare completion, time, and tokens with equivalent Rust/Python tasks; retain the broader CGx2 validation plan's correctness-first gates.

References: [native ABI and C++ facade](https://learn.microsoft.com/dotnet/standard/native-interop/abi-support), [WebAssembly canonical ABI](https://component-model.bytecodealliance.org/advanced/canonical-abi.html), [Python embedding](https://docs.python.org/3.13/extending/embedding.html), [LibTorch stable ABI](https://docs.pytorch.org/docs/stable/notes/libtorch_stable_abi.html), [DLPack](https://dmlc.github.io/dlpack/latest/c_api.html), [Arrow C Data](https://arrow.apache.org/docs/format/Intro.html).
