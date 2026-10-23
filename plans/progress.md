# CGx2 progress estimate after Wave 18 review

**92% of the first usable release implementation scope.** The estimate is unchanged because Wave 18 mainly strengthened correctness, observability, and reproduction for existing slices. It is an inventory judgment, not elapsed time, benchmark performance, or a passed release gate. The OCaml-scale long-term platform goal remains below 10%; a broad standard library, ecosystem, debugger, language server, mature optimizer, and compatibility record are missing.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 29 | Interpreter and native failure reports now expose actual runtime root census; a bounded direct-runtime model checks roots, cycles, regions, nested `no_gc`, and reserve faults. Safe source coverage, sanitizer breadth, foreign lifetimes, concurrency, and the memory release gate remain open. |
| Core language and native toolchain | 20 | 19 | Native debug/release builds show root-census parity for the bounded memory-function profile. Typed object IR, structured values, broader function signatures, and full source semantics remain open. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls execute in a native package. Richer module APIs, ecosystem resolution, and larger source graphs remain open. |
| Library access and source translation | 10 | 9 | A copied, pinned LibTorch CPU package passed independently rerun tensor create/add/shape/copy/release without `libpython`. It remains one local Torch 2.11 C++ route, not generic Python-library compatibility; broader tensor types, portable packages, and source translation remain open. |
| Native notebook | 10 | 9 | `.cgnb` pure function definitions now invalidate transitive dependent outputs on deletion through kernel, CLI, and UI. Effectful persistent workflows, imported libraries, and broader Jupyter-scale execution remain open. |
| Effects, contracts, proof, security | 10 | 9 | The verifier binds proof status to one selected obligation and checks solver executable digests and hostile results. No live trusted Z3/CVC5 result or full memory/foreign proof has run. |
| LLM and runtime evidence | 10 | 8 | Public-only six-task exports and fail-closed trace intake exist; coordinator review fixed a path that could label fabricated trace rows genuine. There are zero genuine paired model trials, no independent human task review, and no scored isolation on this host. |
| **Total** | **100** | **92** | Memory correctness and several first-release capabilities still gate release. |

All six Wave 18 worker commits are integrated on `wave18/integration`; combined Rust, 102 benchmark, UI loopback, format, Clippy, readiness, and real local LibTorch tests pass after coordinator fixes. A passing local C++ route does not establish universal library access. A zero root census does not prove absence of invalid memory access. A validated trace schema does not authenticate a provider call. The measured CPU Auto policy still covers one pure map primitive, not arbitrary `.cg` or GPU code. Wave 19 plans add no implementation credit until reviewed.
