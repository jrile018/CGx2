# CGx2 progress estimate after Wave 9

**72% of the first usable release.** This is a judgment-based scope estimate, not a benchmark score or a passed release gate. The OCaml-scale long-term platform goal remains below 10%: the standard library, ecosystem, debugger, language server, mature compiler, and compatibility record are still missing.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 23 | Source memory and conversions execute; repeated 2,000-seed and extended 10,000-seed runtime audits pass. Failure ordering, full foreign boundary, typed objects, concurrency and deeper audit remain open. |
| Core language and native toolchain | 20 | 15 | Checked scalar `.cg`, source effects/capabilities/contracts, and a manifest-listed imported scalar package execute; native emitted contracts and managed memory remain incomplete. |
| Modules and code understanding | 10 | 9 | Acyclic local imports and public scalar calls check and execute in a native package. Ecosystem dependency resolution and richer module APIs remain open. |
| Library access and source translation | 10 | 4 | Declared CPython host calls exist, but `.cg` cannot yet call them; direct native LibTorch route has no implementation or real test. |
| Native notebook | 10 | 8 | Local UI marks dependent and saved outputs stale with provenance. Rerun still replays a pure-scalar prefix; persistent side effects and richer output remain open. |
| Effects, contracts, proof, security | 10 | 6 | Source declarations and bounded runtime contracts now check; native/Python emission parity, trusted SMT proof, and broader capability enforcement remain open. |
| LLM and runtime evidence | 10 | 7 | Six-case native scalar parity pilot and paired holdout protocol exist, with no genuine paired model traces or general application win. One locally measured CPU map choice is not general parallel execution. |
| **Total** | **100** | **72** | Memory correctness and multiple first-release features still gate release. |

Credit reflects integrated, tested behavior on `wave9/integration`; it is not a fraction of elapsed time. A standalone component earns partial credit, while a language-level claim requires source-to-runtime evidence. The measured CPU choice covers one pure map primitive and machine, not arbitrary `.cg` code or GPU. The hard memory gate cannot be traded for speed or syntax brevity.
