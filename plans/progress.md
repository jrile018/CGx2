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
| [26](wave26/README.md) | 2/2 | Imported notebook public records and conditional source/native memory with scoped cleanup |
| [27](wave27/README.md) | 2/2 | Transitive notebook import closure and bounded source/native memory range loops |
| [28](wave28/REVIEW.md) | 2/2 | Checked transitive imported effects/capabilities and notebook local accumulators/literal ranges |

These are **20 completed bounded deliveries across Waves 22–28**, including evidence work. They are not 20 closed release gates. The Wave 24 loader deliverable identified the bottleneck and did not improve performance.

## Release acceptance remains incomplete

| Area | Implemented and checked | Remaining acceptance work |
| --- | --- | --- |
| Memory | Managed/owned/manual regions, no_gc budgets, roots/graphs, finite fault/pressure models, conditional joins, repeatable memory range bodies and supported source/native parity | Leak-enabled instrumentation on a compatible host; broader ownership/FFI coverage and full supported-semantics safety audit |
| Language/native toolchain | Checked scalars, loops, immutable local records, bounded if/else, CLI and native builds | Variants, Option/Result, generics, richer standard types/libraries and complete semantic integration; broader ownership/iterator integration remains incomplete |
| Modules | Acyclic explicit imports, public scalar/nominal-record APIs and notebook transitive closure with visibility checks | Richer interfaces and ecosystem/package-resolution breadth |
| Foreign libraries/emission | Real direct CPU f32/f64 routes, native process without Python, explicit scalar Python child, supported Rust/Python emitters | Broader typed tensors/libraries, general foreign packaging and cross-target profile coverage; loader validation cost remains high |
| Notebooks | Pure scalar/record functions, transitive imported public records, conditional function bodies, source/root/canonical-path provenance, stale dependencies, persistence/CLI/UI | Effectful workflows and broader retained execution |
| Effects/contracts/security | Static effect/capability checks, checked runtime contracts, return-only conditional extraction, independent finite audit and bounded solver trust boundary | Trusted live solver evidence, broader proofs and analyzer coverage |
| Empirical value | Reproducible selected native/matrix/worker observations with provenance and failure accounting | General runtime/inference/resource targets, scored isolation, independently reviewed paired LLM/human trials |

Passing tests on the implemented subset does not close the other cells. External blockers are explicit: this host blocks LeakSanitizer through ptrace restrictions, has no Z3/CVC5 executable on PATH, and lacks required delegated cgroup controllers. No permission or dependency availability is assumed.

## Latest integrated verification

[Wave 28 review](wave28/REVIEW.md) on `riley/release-completion-plan`: **878 Rust tests passed, five ignored; 23 HTTP tests included; 122 benchmark tests; formatting, Clippy and six public readiness checks pass.** The obsolete notebook rejection assertion was replaced with successful transitive-wrapper/local-loop execution and provenance checks. Fresh release native/Python/direct-f32 boundary checks pass. Leak-enabled, live-solver and scored-isolation gates remain open.

## Current completion plan

Wave 28 integration passes 878 Rust tests (including 23 HTTP), 122 Python tests, Clippy, formatting, 6 readiness checks and fresh release native/Python/direct-f32 boundary acceptance. Five ignored tests remain ignored. There are 20 reviewed bounded deliveries across Waves 22–28, not 20 closed release gates. The [finite release backlog](release-completion/README.md) defines 12 ready chats, four dependency stages, 15 release requirement rows and explicit external evidence gates. No completion percentage is asserted.

## Stage 1 parallel completion review

All twelve implementation chats and three bounded repairs are integrated and reviewed. Current combined checks: 1032 Rust tests passed (six ignored), 166 benchmark tests, 20 docs harness tests, Clippy and formatting. Executable docs have 39 passes, no failures, six blocked and seven future cases. A cleanup-test scheduling race was repaired with a bounded strict descendant-exit check. S2a is ready for one shared language/type owner; parallel consumer work resumes after reviewed interfaces. No release percentage or external gate closure is inferred. See [review](release-completion/s1-review/README.md).

## S2a language/type review

S2a delivered variants, matching and typed Option/Result with 1059 recorded passing Rust tests. Coordinator review reproduced missing match-return inference, package record native type qualification, and misleading/incomplete human API output. Three bounded independent repairs are required; S2b/S3 remain gated. See [review and repair plans](release-completion/s2a-review/README.md). No release gate closure is inferred.

## S2a repaired and reviewed

The three fixes pass independent review and combined 1078 Rust tests (six ignored), 166 benchmark tests, 20 documentation harness tests and 39 runnable docs. Original failure repros now pass; API checks preserve unchecked syntax-only status. S2a is accepted as a bounded language/type delivery. S2b foundation is ready, with ordinary memory/foreign/execution/command integrations still explicitly required. External release gates and the absence of a completion percentage are unchanged.

## S2b foundation review

Foundation delivery 414c71f is staged repair-required. Verified worker evidence reports 1092 passing Rust tests and 159 product checks, but coordinator reproductions found seven defects in semantic resolution, package binder qualification, emitted targets, package output bounds and API visibility. Five disjoint bounded repairs are ready; see [review](release-completion/s2b-review/README.md). No R02/R10 closure or release percentage; remaining S2b and external evidence stay open.

## S2b first repairs integrated

Five bounded fixes are reviewed and integrated. Combined1126 Rust tests (six ignored), Clippy, formatting, binaries and159 foundation product checks pass. Expanded tests still reproduce native constructor dispatch and two source-specialization defects; two exclusive repair plans address them. R02/R10 foundation acceptance remains failed, with the full remaining S2b, S3 and external evidence ledger preserved. See [review](release-completion/s2b-review/README.md).
