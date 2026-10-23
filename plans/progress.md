# CGx2 progress estimate after Wave 20 review

**92% of the tracked first-release implementation scope.** This is a coarse feature inventory, not a probability of release readiness or a benchmark result. Wave 20 added useful tested paths, but the remaining points represent hard gates that did not close. The longer-term OCaml-scale platform goal remains below 10%; the standard library, ecosystem, debugger, language server, mature optimizer, and compatibility record are still small.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 29 | Wave 20 added source/native memory parity fixtures and scoped generated-process AddressSanitizer evidence. Leak detection, full source coverage, foreign lifetimes, parallel ownership, and the memory release gate remain open. |
| Core language and native toolchain | 20 | 19 | Checked scalar loops and a bounded source-level matrix call run; the matrix call remains a lexical string-literal profile. Typed object IR, structured values, broader function signatures, and complete source semantics remain open. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls execute in native packages. Larger source graphs, richer APIs, and ecosystem resolution remain open. |
| Library access and source translation | 10 | 9 | A `.cg` matrix call reached the pinned LibTorch CPU facade directly in a real local test; acyclic scalar packages and checked range loops translate to Python. General Python-library compatibility, broad tensor types, and portable foreign packages remain open. |
| Native notebook | 10 | 9 | Explicit digest-referenced markdown images survive native/portable save and clean sweeps. The coordinator fixed overwrite-save after deleting images. Effectful workflows, package imports, and broader Jupyter-scale execution remain open. |
| Effects, contracts, proof, security | 10 | 9 | Bounded effects/contracts/security checks and solver trust validation exist. No live trusted Z3/CVC5 proof run or full memory/foreign proof has occurred. |
| LLM and runtime evidence | 10 | 8 | Public syntax pilot and conservative CPU policy measurements exist. There are zero genuine paired model trials, no independent human task review or scored isolation on this host, and no general `.cg` parallel or GPU lowering. |
| **Total** | **100** | **92** | Memory correctness and other first-release acceptance gates still block release. |

All six Wave 20 worker commits were integrated on `wave20/integration`. Combined offline Rust workspace tests, notebook UI loopback tests, 102 Python benchmark tests, readiness with six reference checks, formatting, and workspace Clippy with `-D warnings` pass after the coordinator's notebook-save and frozen-hash fixes. A real `.cg` 2x3 by 3x2 matrix example passed through the pinned Torch 2.11 CPU package in 163.21 seconds with no Python middleman. The worker's separate real facade boundary test and mock sanitizer run are documented, but the real facade was not sanitizer-instrumented. This local result does not establish broad PyTorch compatibility or automatic GPU selection.
