# CGx2: ten high-value library ecosystems

Status: design and execution roadmap, 2026-10-22. This document does **not** claim that any listed ecosystem is already callable from `.cg` source.

Implementation tasks are in `docs/superpowers/plans/2026-10-22-cgx2-top10-ecosystems.md`, after the common foundation in `docs/superpowers/plans/2026-10-22-cgx2-interoperability.md`.

## Selection rule

The ten languages are the ten highest non-markup, non-query, non-shell entries in the [2026 Stack Overflow Developer Survey's all-respondent usage table](https://survey.stackoverflow.co/2026/technology/data/languages): JavaScript, Python, TypeScript, Java, C#, C++, C, Rust, PHP, and Go. This is a usage-based shortlist, **not** a verified ranking by number of libraries. Package registries count different things, and JavaScript and TypeScript mostly share npm packages. Revisit the shortlist when usage and user demand change.

CGx2 supports a **package** only after its particular interface, version, runtime requirements, and ownership rules are declared and tested. Supporting a language's route does not automatically support every package written in that language.

## Common package experience

An explicit `cgx2 bind add ECOSYSTEM:PACKAGE@VERSION` command should ask the ecosystem's existing package manager to resolve and install the package, then record the exact artifact, transitive dependency lock, route, adapter version, supported platform, and reviewed declaration in `cgx2.toml` and a CGx2 lockfile. The CLI must never install packages merely because source imports a name. A human can review a generated binding before it becomes trusted. `cgx2 bind check` validates the installed artifact, required symbols/exports, runtime, types, effects, and ownership; it reports unavailable features without invoking package code where the route allows. Generated `.cg` facades and machine-readable API information make the imported package discoverable to humans and LLMs.

Native routes stay in-process. Runtime and worker routes are explicit. A package that exposes both native and runtime APIs may offer two separately named bindings; CGx2 does not silently switch routes. PyTorch's primary binding is **LibTorch**, independent of the Python package route.

## Ecosystem matrix

| Language | Package ecosystem | First useful CGx2 route | What is still required per package | Representative acceptance target |
| --- | --- | --- | --- | --- |
| JavaScript | npm | Persistent, version-pinned Node worker with typed request/response calls | Export map, asynchronous result and exception mapping, serialization limits, declared effects | Call `lodash.camelCase`; reject an undeclared export |
| Python | PyPI | Explicit in-process CPython binding for Python-only packages; native API when a package publishes one | Interpreter/environment lock, callable signature, Python object lifetime and exception mapping | Call `python-dateutil` parsing; direct `torch` tests must use LibTorch without CPython |
| TypeScript | npm | Same Node worker as JavaScript after the package's supported JavaScript build is resolved | Use `.d.ts` only as a candidate type source; review runtime exports, erased types, effects, and ownership | Call a compiled `date-fns` export through a typed `.cg` facade |
| Java | Maven Central | In-process JVM Invocation API/JNI adapter | Classpath lock, Java signature, object references, thread attachment, exceptions | Call Apache Commons Lang `StringUtils.capitalize` |
| C# | NuGet | In-process .NET `hostfxr` adapter for compatible framework-dependent assemblies | Runtime configuration, assembly identity, managed entrypoint facade, GC handle and exception policy | Call Newtonsoft.Json serialize through a reviewed managed facade |
| C++ | Conan/vcpkg or supplied build | Small package-specific C ABI facade compiled against the selected C++ toolchain | Resolve overloads/templates, exception translation, object destroy function, C++ ABI/version | Call `fmt::format` through a C facade |
| C | Supplied library/system/Conan/vcpkg | Direct checked C ABI FFI | Header-to-declaration review, width/alignment, ownership, error convention | Round-trip data through zlib compression/decompression |
| Rust | crates.io | Build a small Rust `extern "C"` facade around selected public crate APIs | Define stable C-facing types, panic boundary, ownership and compiler/version lock | Use `regex` through an opaque compiled handle |
| PHP | Packagist/Composer | Persistent, version-pinned PHP worker with typed request/response calls | Composer lock, autoload/bootstrap, object/value conversion, exceptions and effects | Call a selected `ramsey/uuid` operation and return its text value |
| Go | Go modules | Reviewed `-buildmode=c-shared` facade where supported; process worker otherwise | Explicit exported functions, Go runtime/thread/pointer constraints, error and ownership policy | Call a wrapper around `golang.org/x/text` and reject an unsupported platform |

These are route choices, not claims of full API parity. A Node or PHP worker has serialization and call overhead. JVM and .NET adapters load those runtimes; CPython runs for Python-only APIs. A C++ or Rust package without a compatible exported boundary needs its small native facade. Go `c-shared` exports only explicitly marked functions. Official interface references: [Java JNI Invocation API](https://docs.oracle.com/en/java/javase/26/docs/specs/jni/invocation.html), [.NET hosting](https://learn.microsoft.com/en-us/dotnet/core/tutorials/netcore-hosting), [Rust FFI](https://doc.rust-lang.org/stable/nomicon/ffi.html), [Go `c-shared`](https://go.dev/cmd/go/), [TypeScript/JavaScript relationship](https://www.typescriptlang.org/docs/handbook/typescript-in-5-minutes.html).

## Six reusable engines, ten language routes

1. **Checked native ABI engine:** C directly; C++, Rust, and suitable Go packages through reviewed C-facing facades; direct LibTorch lives here as its own package adapter.
2. **CPython engine:** only Python packages whose selected callable interface requires Python.
3. **Node engine:** one worker protocol for JavaScript and TypeScript npm packages.
4. **JVM engine:** Java Maven packages through a pinned classpath and JNI.
5. **.NET engine:** C# NuGet assemblies through `hostfxr` and a reviewed entrypoint facade.
6. **Generic worker engine:** PHP first, Go fallback, and other ecosystems whose runtime cannot safely be hosted in-process or whose interface is service-oriented.

The worker protocol is versioned, length-framed, authenticated to the local child process, bounded in message size and time, and distinguishes remote exception, timeout, crash, and protocol error. Passing arrays/tensors between processes is a copy unless a separately reviewed shared-memory protocol proves safe ownership and synchronization. A worker must never acquire unlisted filesystem/network capabilities merely because its host language allows them.

## Rollout and review gates

**Foundation.** Finish `docs/superpowers/plans/2026-10-22-cgx2-interoperability.md` Tasks 1–4 after Wave 9 review: generic declaration, typed source checking, C/native FFI, memory boundary, direct LibTorch. A `.cg` source fixture must run; a host-side Rust API alone does not count.

**Package resolution.** Add the explicit `bind add/check` workflow and lockfile. Verify offline rebuild from pinned artifacts; reject dependency drift and incompatible platform/runtime before a user call. Native C/C++ packages may require a supplied artifact rather than a registry.

**Runtime engines.** Implement CPython source calls, Node, JVM, .NET, and worker protocol as separate reviewed changes. The Python engine must not become the PyTorch path. Add PHP on the worker engine and Go's optional `c-shared` facade. Use one minimal hand-authored fixture and one real registry package per ecosystem.

**Compatibility evidence.** For each of the ten rows, require: a `.cg` program using the representative package; a negative wrong-type/unsupported-version test; correct owner/destructor or runtime-handle behavior; an effect/capability test; a clean/offline lockfile test; and a measured cold start, tiny-call latency, bulk throughput, copies, and peak memory against the ecosystem's own direct caller. Report supported OS/architecture/runtime versions. Run held-out LLM tasks that import unfamiliar packages using the structured API index; measure verified completion and total time/tokens against language-native solutions.

**Claim threshold.** Report coverage as `N/10 ecosystem routes with end-to-end .cg acceptance` and `M tested packages`, never as “all libraries supported.” Each package is available only within its tested API subset and platform matrix. An adapter engine can exist before a particular package passes acceptance; those are separate milestones.
