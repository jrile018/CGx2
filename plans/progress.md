# CGx2 progress estimate after Wave 12

**84% of the first usable release implementation scope.** This is a judgment-based scope estimate, not a benchmark score or a passed release gate. Memory correctness, full functional conformance, LLM value, and runtime value remain open release decisions. The OCaml-scale long-term platform goal remains below 10%: the standard library, ecosystem, debugger, language server, mature compiler, and compatibility record are still missing.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 26 | Direct runtime object slots/graph tracing and finite state-model tests add coverage. `.cg` graph operations, native collection-count parity, host allocator failure injection, foreign lifetimes, concurrency and deeper audit remain open. |
| Core language and native toolchain | 20 | 18 | Checked local memory functions now build natively in debug/release. The profile remains zero-argument/Unit and lacks typed object IR, native memory reports and broader source coverage. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls check and execute in a native package. Ecosystem dependency resolution and richer module APIs remain open. |
| Library access and source translation | 10 | 7 | One fixed-shape pinned C call runs from explicit unsafe `.cg` source without Python. Binding hash/loader aliasing and static-check execution defects were repaired; source LibTorch, arbitrary packages and safe physical ABI proof remain open. |
| Native notebook | 10 | 8 | Pure scalar cells retain state and later execution does not replay earlier source. Side-effecting cells, rich portable output and Jupyter-scale features remain open. |
| Effects, contracts, proof, security | 10 | 9 | Checked `.cg` pure contracts reach finite verification with source spans and honest global-unknown status. Broad proof, stronger capability enforcement and a trusted live solver remain open. |
| LLM and runtime evidence | 10 | 7 | Auditable task/run importer and clustered holdout analysis exist, but tasks are candidates and no genuine paired model traces or general application win exist. |
| **Total** | **100** | **84** | Memory correctness and multiple first-release features still gate release. |

Credit reflects integrated, tested behavior on `wave12/integration`; it is not a fraction of elapsed time. A standalone component earns partial credit, while a language-level claim requires source-to-runtime evidence. Real LibTorch, trusted solver, and model holdout gates have not run. The measured CPU choice covers one pure map primitive and machine, not arbitrary `.cg` code or GPU. The hard memory gate cannot be traded for speed or syntax brevity.
