# CGx2 progress estimate after Wave 15 review

**90% of the first usable release implementation scope.** This is a judgment-based inventory estimate, not a benchmark score or a passed release gate. Memory correctness, full functional conformance, LLM value, and runtime value remain open release decisions. The OCaml-scale long-term platform goal remains below 10%: the standard library, ecosystem, debugger, language server, mature compiler, and compatibility record are still missing.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 29 | Wave 15 exercised automatic collection through actual thresholds in a bounded direct-runtime corpus and added source/native failure reports. Root-count and cleanup parity, foreign lifetimes, dynamic-tool and concurrency evidence remain open; the memory release gate has not passed. |
| Core language and native toolchain | 20 | 19 | Native debug/release builds handle the bounded local object graph and operation-local failure reporting. Typed object IR, broader function signatures, and final-unwind report parity remain open. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls check and execute in a native package. Ecosystem dependency resolution and richer module APIs remain open. |
| Library access and source translation | 10 | 8 | A bounded unsafe `.cg` CPU float64 tensor call uses a pinned ELF closure without Python. Wave 15 improved preloaded-module checks and bounded copy, but real LibTorch execution, general tensor values and broad library access remain open. |
| Native notebook | 10 | 9 | `.cgnb` output and selective rerun now have bounded restart/interrupt behavior. External side effects and broader Jupyter-scale execution remain open. |
| Effects, contracts, proof, security | 10 | 9 | Checked `.cg` pure contracts reach finite verification with source spans and honest global-unknown status. Wave 15's trusted proof lane delivered no commit; live solver evidence remains open. |
| LLM and runtime evidence | 10 | 7 | Candidate scoring fails closed when namespace isolation is unavailable and pins the Rust compiler hash. Human task review, aggregate resource containment, live isolated scoring, and paired model trials remain absent. |
| **Total** | **100** | **90** | Memory correctness and multiple first-release features still gate release. |

Credit reflects integrated, tested behavior on `wave15/integration`; it is not a fraction of elapsed time. Five Wave 15 worker commits were integrated and coordinator review fixed a loader filename blind spot and notebook interrupt stalls. A standalone component earns partial credit, while a language-level claim requires source-to-runtime evidence. Real LibTorch, trusted solver, and model holdout gates have not run. The measured CPU choice covers one pure map primitive and machine, not arbitrary `.cg` code or GPU. The hard memory gate cannot be traded for speed or syntax brevity.
