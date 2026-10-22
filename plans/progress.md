# CGx2 progress estimate after Wave 13

**87% of the first usable release implementation scope.** This is a judgment-based scope estimate, not a benchmark score or a passed release gate. Memory correctness, full functional conformance, LLM value, and runtime value remain open release decisions. The OCaml-scale long-term platform goal remains below 10%: the standard library, ecosystem, debugger, language server, mature compiler, and compatibility record are still missing.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 27 | Checked local `.cg` managed references and runtime allocation-fault injection add coverage. Native source graph compilation, full fault/stress audit, foreign lifetimes and concurrency remain open. |
| Core language and native toolchain | 20 | 19 | Native functions now report actual memory/collection outcomes. Checked source graph operations still lack a native path; typed object IR and broader function signatures remain open. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls check and execute in a native package. Ecosystem dependency resolution and richer module APIs remain open. |
| Library access and source translation | 10 | 7 | A direct CPU LibTorch facade prototype has explicit tensor copy/handle rules. Only the facade `.so` is hashed; dynamic LibTorch dependencies, direct `.cg` tensor calls, a real run and broad library access remain open. |
| Native notebook | 10 | 9 | Pure scalar cells retain state, and self-contained `.cgnb` export reopens bounded assets. Captured effectful output, external side effects and Jupyter-scale features remain open. |
| Effects, contracts, proof, security | 10 | 9 | Checked `.cg` pure contracts reach finite verification with source spans and honest global-unknown status. Broad proof, stronger capability enforcement and a trusted live solver remain open. |
| LLM and runtime evidence | 10 | 7 | Two executable task candidates and hidden workspace acceptance exist. They are not independently reviewed or frozen, and no genuine paired model traces or general application win exist. |
| **Total** | **100** | **87** | Memory correctness and multiple first-release features still gate release. |

Credit reflects integrated, tested behavior on `wave13/integration`; it is not a fraction of elapsed time. A standalone component earns partial credit, while a language-level claim requires source-to-runtime evidence. Real LibTorch, trusted solver, and model holdout gates have not run. The measured CPU choice covers one pure map primitive and machine, not arbitrary `.cg` code or GPU. The hard memory gate cannot be traded for speed or syntax brevity.
