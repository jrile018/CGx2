# CGx2 progress estimate after Wave 10

**76% of the first usable release.** This is a judgment-based implementation-scope estimate, not a benchmark score or a passed release gate. Memory correctness, full functional conformance, LLM value, and runtime value remain open release decisions. The OCaml-scale long-term platform goal remains below 10%: the standard library, ecosystem, debugger, language server, mature compiler, and compatibility record are still missing.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 24 | Allocation failure ordering and reclaim paths now have regressions, and bounded memory scripts run natively. Memory in functions, typed objects, full foreign lifetime integration, concurrency and deeper audit remain open. |
| Core language and native toolchain | 20 | 16 | Checked scalar `.cg`, source contracts and native emitted contracts run; bounded memory scripts build natively, but the compiler requires its source checkout and broader typed IR is absent. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls check and execute in a native package. Ecosystem dependency resolution and richer module APIs remain open. |
| Library access and source translation | 10 | 5 | `.cg` calls a restricted Python-only scalar subset; native C ABI fixture works through an explicitly unsafe reviewed host API. Direct LibTorch facade is untested against real LibTorch and not callable from `.cg`. |
| Native notebook | 10 | 8 | Local UI marks dependent and saved outputs stale with provenance. Rerun still replays a pure-scalar prefix; persistent side effects and richer output remain open. |
| Effects, contracts, proof, security | 10 | 7 | Source contracts now survive supported Rust/Python emission and unsupported foreign output is rejected. Trusted SMT proof and broader file/process/network capability enforcement remain open. |
| LLM and runtime evidence | 10 | 7 | Six-case native scalar parity pilot and paired holdout protocol exist, with no genuine paired model traces or general application win. One locally measured CPU map choice is not general parallel execution. |
| **Total** | **100** | **76** | Memory correctness and multiple first-release features still gate release. |

Credit reflects integrated, tested behavior on `wave10/integration`; it is not a fraction of elapsed time. A standalone component earns partial credit, while a language-level claim requires source-to-runtime evidence. Real LibTorch, trusted solver, and model holdout gates have not run. The measured CPU choice covers one pure map primitive and machine, not arbitrary `.cg` code or GPU. The hard memory gate cannot be traded for speed or syntax brevity.
