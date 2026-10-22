# CGx2 progress estimate after Wave 11

**80% of the first usable release implementation scope.** This is a judgment-based scope estimate, not a benchmark score or a passed release gate. Memory correctness, full functional conformance, LLM value, and runtime value remain open release decisions. The OCaml-scale long-term platform goal remains below 10%: the standard library, ecosystem, debugger, language server, mature compiler, and compatibility record are still missing.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 25 | Local zero-argument Unit memory functions run in one session and v5 finite state-machine audits pass. Typed object graphs, full foreign lifetimes, concurrency, sanitizers and deeper audit remain open. |
| Core language and native toolchain | 20 | 17 | Bounded memory scripts build natively with a portable compiler and optimized release runtime. New memory functions lack native output; broader typed IR is absent. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls check and execute in a native package. Ecosystem dependency resolution and richer module APIs remain open. |
| Library access and source translation | 10 | 6 | Pinned native binding preflight checks bounded lock/artifact bytes, symbols and target, while C fixture remains an explicitly unsafe host API. `.cg` cannot call it or direct LibTorch; real LibTorch is unavailable here. |
| Native notebook | 10 | 8 | Local UI marks dependent and saved outputs stale with provenance. Rerun still replays a pure-scalar prefix; persistent side effects and richer output remain open. |
| Effects, contracts, proof, security | 10 | 8 | A solver-supplied counterexample is labeled disproved only after independent checked evaluation. UNSAT text remains untrusted; `.cg` source contract extraction and broader capability enforcement remain open. |
| LLM and runtime evidence | 10 | 7 | Six-case native scalar parity pilot and paired holdout protocol exist, with no genuine paired model traces or general application win. One locally measured CPU map choice is not general parallel execution. |
| **Total** | **100** | **80** | Memory correctness and multiple first-release features still gate release. |

Credit reflects integrated, tested behavior on `wave11/integration`; it is not a fraction of elapsed time. A standalone component earns partial credit, while a language-level claim requires source-to-runtime evidence. Real LibTorch, trusted solver, and model holdout gates have not run. The measured CPU choice covers one pure map primitive and machine, not arbitrary `.cg` code or GPU. The hard memory gate cannot be traded for speed or syntax brevity.
