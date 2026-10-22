# CGx2 progress estimate after Wave 8

**68% of the first usable release.** This is a judgment-based scope estimate, not a benchmark score or a passed release gate. The OCaml-scale long-term platform goal remains below 10%: the standard library, ecosystem, debugger, language server, mature compiler, and compatibility record are still missing.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 22 | Source memory and conversions execute; v1/v2 source corpora and a twice-run 1,000-seed direct-runtime state machine pass. Formal audit, foreign boundary, typed objects, concurrency and broader fuzzing remain open. |
| Core language and native toolchain | 20 | 14 | Checked scalar `.cg` and a manifest-listed imported scalar package build into native executables. Native managed-memory codegen, general typed IR and broad syntax remain absent. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls check and execute in a native package. Ecosystem dependency resolution and richer module APIs remain open. |
| Python access and source translation | 10 | 4 | CPython bridge handles bounded structured values and keyword calls; no `.cg` Python calls or real PyTorch test on this host. |
| Native notebook | 10 | 7 | Local UI reruns a selected pure-scalar cell from a bounded fresh state and marks dependent outputs stale. Persistent side effects and richer output remain open. |
| Effects, contracts, proof, security | 10 | 5 | External finite-domain contract command and advisory source-located effect/taint policy exist. Source declarations, enforcement and trusted solver proofs remain open. |
| LLM and runtime evidence | 10 | 7 | Six-case native scalar parity pilot exists. A local checked i64 map profile conservatively selected four-worker CPU for one 1,048,576-element size; no LLM traces, holdout or general application win exists. |
| **Total** | **100** | **68** | Memory correctness and multiple first-release features still gate release. |

Credit reflects integrated, tested behavior on `wave8/integration`; it is not a fraction of elapsed time. A standalone component earns partial credit, while a language-level claim requires source-to-runtime evidence. The measured CPU choice covers one pure map primitive and machine, not arbitrary `.cg` code or GPU. The hard memory gate cannot be traded for speed or syntax brevity.
