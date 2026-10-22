# CGx2 progress estimate after Wave 14

**89% of the first usable release implementation scope.** This is a judgment-based scope estimate, not a benchmark score or a passed release gate. Memory correctness, full functional conformance, LLM value, and runtime value remain open release decisions. The OCaml-scale long-term platform goal remains below 10%: the standard library, ecosystem, debugger, language server, mature compiler, and compatibility record are still missing.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 28 | Checked source graph operations compile natively and a 64-seed runtime graph/fault model runs. Automatic collection across real thresholds, source/native error reports, foreign lifetimes, sanitizer and concurrency evidence remain open. |
| Core language and native toolchain | 20 | 19 | Native debug/release builds handle the bounded local object graph. Failed executions still lack interpreter/native partial report parity; typed object IR and broader function signatures remain open. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls check and execute in a native package. Ecosystem dependency resolution and richer module APIs remain open. |
| Library access and source translation | 10 | 8 | A bounded unsafe `.cg` CPU float64 tensor call uses a schema-2 pinned ELF closure with no Python bridge. Loader trust, real LibTorch execution, general tensor values and broad library access remain open. |
| Native notebook | 10 | 9 | `.cgnb` export reopens bounded assets and saved text output with selective rerun. Interrupt/restart behavior, external side effects and Jupyter-scale features remain open. |
| Effects, contracts, proof, security | 10 | 9 | Checked `.cg` pure contracts reach finite verification with source spans and honest global-unknown status. Broad proof, stronger capability enforcement and a trusted live solver remain open. |
| LLM and runtime evidence | 10 | 7 | Two task cards and an honest `unrun` evidence packet exist; submitted code execution now fails closed on the coordinator host. Human task review, isolated scoring and paired model trials remain absent. |
| **Total** | **100** | **89** | Memory correctness and multiple first-release features still gate release. |

Credit reflects integrated, tested behavior on `wave14/integration`; it is not a fraction of elapsed time. A standalone component earns partial credit, while a language-level claim requires source-to-runtime evidence. Real LibTorch, trusted solver, and model holdout gates have not run. The measured CPU choice covers one pure map primitive and machine, not arbitrary `.cg` code or GPU. The hard memory gate cannot be traded for speed or syntax brevity.
