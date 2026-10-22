# CGx2 progress estimate after Wave 5

**40% of the first usable release.** This is a judgment-based scope estimate, not a benchmark score or a claim that a release gate has passed. The OCaml-scale long-term platform goal is below 10% because a standard library, package ecosystem, debugger, language server, mature compiler, and sustained compatibility remain open.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | ---: | ---: | --- |
| End-to-end safe memory | 30 | 14 | A bounded `.cg` script profile runs managed/owned regions and `no_gc` through the runtime; no full conformance, conversions in source, or broad source semantics. |
| Core language and native toolchain | 20 | 10 | Parser, scalar checker/CLI, formatter, scalar Rust emitter; no full native build path, typed general IR, or broad syntax. |
| Modules and code understanding | 10 | 4 | Public API and strict local package index; no source imports/effect summaries. |
| Python access and source translation | 10 | 3 | Isolated CPython/PyTorch bridge and Python scalar emitter; no `.cg` imports or real PyTorch test on this host. |
| Native notebook | 10 | 4 | `.cgnb` format/session, pure-scalar replay kernel, and file runner; no interactive UI or persistent side effects. |
| Effects, contracts, proof, security | 10 | 2 | Standalone bounded verifier; no source contract/effect/capability integration or live solver gate. |
| LLM and runtime evidence | 10 | 3 | Harness, six real scalar parity cases, and CPU execution policy prototype; no model traces, large tasks, holdout, or native speed result. |
| **Total** | **100** | **40** | Release remains gated by memory correctness and all first-release features. |

Update this table only after integrated behavior is verified. A completed standalone crate earns partial credit; an advertised feature requires source-to-runtime or source-to-tool end-to-end tests. The hard memory correctness gate cannot be traded for speed or brevity. The revised execution policy in `docs/language-design.md` aims for safe CPU/parallel/GPU choices where backends and measurements justify them; it does not promise global optimality.
