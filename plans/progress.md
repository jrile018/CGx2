# CGx2 progress estimate after Wave 7

**60% of the first usable release.** This is a judgment-based scope estimate, not a benchmark score or a claim that a release gate has passed. The OCaml-scale long-term platform goal is below 10% because a standard library, package ecosystem, debugger, language server, mature compiler, and sustained compatibility remain open.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 20 | Source conversions and 128-seed v1 plus 256-seed v2 deterministic memory corpora run through the runtime; broader fuzz/audit, foreign boundary, and typed objects remain open. |
| Core language and native toolchain | 20 | 13 | Checked scalar `.cg` now builds into a native executable; no full native memory path, typed general IR, or broad syntax. |
| Modules and code understanding | 10 | 7 | Local imports resolve and public scalar calls type-check across an acyclic package graph; imported code does not execute yet. |
| Python access and source translation | 10 | 4 | CPython bridge handles bounded structured values and keyword calls; no `.cg` Python calls or real PyTorch test on this host. |
| Native notebook | 10 | 6 | Local loopback UI edits, runs, and saves pure-scalar `.cgnb`; no persistent side-effecting reruns or full rich output. |
| Effects, contracts, proof, security | 10 | 4 | External contract command returns finite-domain verdicts and untrusted solver observations; no source enforcement or live trusted proof gate. |
| LLM and runtime evidence | 10 | 6 | Six-case native scalar pilot passed parity and recorded 144 launch samples; task-matrix audit exists, but there are no model traces, holdout, or native application benchmark win. |
| **Total** | **100** | **60** | Release remains gated by memory correctness and all first-release features. |

Update this table only after integrated behavior is verified. A completed standalone crate earns partial credit; an advertised feature requires source-to-runtime or source-to-tool end-to-end tests. The hard memory correctness gate cannot be traded for speed or brevity. The revised execution policy in `docs/language-design.md` aims for safe CPU/parallel/GPU choices where backends and measurements justify them; it does not promise global optimality.
