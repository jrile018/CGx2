# CGx2 progress estimate after Wave 6

**52% of the first usable release.** This is a judgment-based scope estimate, not a benchmark score or a claim that a release gate has passed. The OCaml-scale long-term platform goal is below 10% because a standard library, package ecosystem, debugger, language server, mature compiler, and sustained compatibility remain open.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 18 | Source owned/managed conversions and 128-seed deterministic memory corpus run through the runtime; broader fuzz/audit, foreign boundary, and typed objects remain open. |
| Core language and native toolchain | 20 | 13 | Checked scalar `.cg` now builds into a native executable; no full native memory path, typed general IR, or broad syntax. |
| Modules and code understanding | 10 | 5 | Public API, strict package index, and conservative call/effect index; no resolved source imports or executing packages. |
| Python access and source translation | 10 | 3 | Isolated CPython/PyTorch bridge and Python scalar emitter; no `.cg` imports or real PyTorch test on this host. |
| Native notebook | 10 | 6 | Local loopback UI edits, runs, and saves pure-scalar `.cgnb`; no persistent side-effecting reruns or full rich output. |
| Effects, contracts, proof, security | 10 | 3 | Conservative advisory effect index and standalone bounded verifier; no source enforcement or live proof gate. |
| LLM and runtime evidence | 10 | 4 | Harness, six scalar parity cases, deterministic memory corpus, and CPU policy prototype; no model traces, holdout, or native runtime benchmark win. |
| **Total** | **100** | **52** | Release remains gated by memory correctness and all first-release features. |

Update this table only after integrated behavior is verified. A completed standalone crate earns partial credit; an advertised feature requires source-to-runtime or source-to-tool end-to-end tests. The hard memory correctness gate cannot be traded for speed or brevity. The revised execution policy in `docs/language-design.md` aims for safe CPU/parallel/GPU choices where backends and measurements justify them; it does not promise global optimality.
