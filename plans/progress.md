# CGx2 implementation milestones and release gates

## Why the percentage stayed at 92%

The old weighted estimate gave most areas all but their final point, then withheld that point until several large acceptance requirements closed. As a result, new conditionals, Python isolation and notebook record functions could land without changing the total. It was too coarse to measure incremental delivery and too optimistic about the remaining work. **92% is a retired historical estimate, not a current release-completion claim.** There is no evidence-based replacement percentage yet; changing the denominator or adding points after each wave would invent precision.

## Reviewed work is advancing

| Wave | Reviewed lane deliveries | Concrete additions |
| --- | ---: | --- |
| [22](wave22/README.md) | 5/5 | Region failure model, f32 source route, notebook records, Python record emission, larger matrix observations |
| [23](wave23/README.md) | 4/4 | Automatic collection under pressure, Python-free native process with explicit worker, notebook record functions, optimized f32 observations |
| [24](wave24/README.md) | 3/3 | Conditional statements, measured loader bottleneck with checks preserved, native/Python-worker paired observations |
| [25](wave25/README.md) | 2/2 | Public nominal records across package modules and finite/source/SMT conditional contract verification |

These are **14 completed bounded deliveries across Waves 22–25**, including evidence work. They are not 14 closed release gates. The Wave 24 loader deliverable identified the bottleneck and did not improve performance.

## Release acceptance remains incomplete

| Area | Implemented and checked | Remaining acceptance work |
| --- | --- | --- |
| Memory | Managed/owned/manual regions, no_gc budgets, roots/graphs, finite fault/pressure models, supported source/native parity | Leak-enabled instrumentation on a compatible host; broader ownership/FFI coverage and full supported-semantics safety audit |
| Language/native toolchain | Checked scalars, loops, immutable local records, bounded if/else, CLI and native builds | Variants, Option/Result, generics, richer standard types/libraries and complete semantic integration; conditional memory joins remain rejected |
| Modules | Acyclic explicit imports and public scalar/nominal-record function APIs with visibility checks | Richer interfaces, ecosystem/package-resolution breadth and notebook consumption of public records |
| Foreign libraries/emission | Real direct CPU f32/f64 routes, native process without Python, explicit scalar Python child, supported Rust/Python emitters | Broader typed tensors/libraries, general foreign packaging and cross-target profile coverage; loader validation cost remains high |
| Notebooks | Pure scalar/local-record functions, imports in the supported scalar profile, conditional function bodies, stale dependencies, persistence/CLI/UI | Imported record values, effectful workflows and broader retained execution |
| Effects/contracts/security | Static effect/capability checks, checked runtime contracts, return-only conditional extraction, independent finite audit and bounded solver trust boundary | Trusted live solver evidence, broader proofs and analyzer coverage |
| Empirical value | Reproducible selected native/matrix/worker observations with provenance and failure accounting | General runtime/inference/resource targets, scored isolation, independently reviewed paired LLM/human trials |

Passing tests on the implemented subset does not close the other cells. External blockers are explicit: this host blocks LeakSanitizer through ptrace restrictions, has no Z3/CVC5 executable on PATH, and lacks required delegated cgroup controllers. No permission or dependency availability is assumed.

## Latest integrated verification

Wave 25 on `riley/wave25-review`: **778 Rust tests passed, five ignored; 20 HTTP tests; 122 benchmark tests; formatting, Clippy and six public readiness checks pass.** Independent release real f32 execution/native Python isolation and conditional finite-verifier smoke pass. The compiler pilot subtree pin was refreshed. See [review and evidence](wave25/README.md).

## Next bounded capabilities

[Wave 26](wave26/README.md) targets imported public records in pure notebooks and conditional memory execution with conservative ownership joins. These use already committed package/type/conditional interfaces. Workers commit independently for review; acceptance criteria are fixed before work, with no completion credit for plans.
