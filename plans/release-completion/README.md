# CGx2 finite completion plan
Status: ordinary-memory API, admission including nominal constructor resolution, and bounded runtime-root repairs reviewed. Runtime allocation recovery and evidence-harness hardening chats remain active. Prior combined1360Rust and quality gates pass; full integration of these follow-ups awaits their deliveries. Full ordinary ownership/transfer/v3, foreign/execution/commands, S3/S4 and real acceptance gates remain open. Main unmerged. See [current review](s2b-ordinary-memory-review/README.md).

## Answer to “how many more waves?”

There are **four dependency stages**, with **12 independent chats ready in Stage 1**. These are dependency stages, not a promise of four short coding waves. Stage 2 contains the shared language and compiler work; Stage 3 reconnects independent consumers; Stage 4 runs the actual acceptance gates. The critical path is S1 → S2a → S2b → S3 → S4. Work may require repair iterations, but every remaining requirement below has a named owner, exit condition and dependency. Do not invent additional waves without linking them to an unmet acceptance row.

Planning range: S1 is one substantial deliverable per chat; S2 comprises the reviewed language/type delivery, executable generic/data foundation, and the remaining named source integration checkpoints; S3 is one integration delivery per applicable consumer/adapter; S4 is one release-candidate evidence/review cycle plus repairs tied to failing checks. There is no defensible calendar ETA until S1 outputs and external runner/study availability are known. Budget roughly 20–30% of implementation effort for cross-component integration and review. No “92%” metric or completed-chat count substitutes for release gates.

## Frozen baseline and immediate dispatch

Wave 28 package effects and notebook local loops are integrated on this branch. See [review](../wave28/REVIEW.md). Shared contracts are frozen in [contracts.md](contracts.md); all workers follow [worker-rules.md](worker-rules.md). Each chat owns only the paths in its plan and commits one coherent delivery for review.

| ID | Ready delivery | Plan |
| --- | --- | --- |
| S1-01 | release core commands | [core](core/plan.md) |
| S1-02 | fallible runtime and lifetime completion | [memory](memory/plan.md) |
| S1-03 | ownership analysis for application functions | [ownership](ownership/plan.md) |
| S1-04 | native binding SDK and direct inference | [native](native/plan.md) |
| S1-05 | pinned Python package runtime | [python](python/plan.md) |
| S1-06 | persistent worker transport | [worker](worker/plan.md) |
| S1-07 | cancellable CPU execution and profiling | [execution](execution/plan.md) |
| S1-08 | retained notebook runtime contracts | [notebook](notebook/plan.md) |
| S1-09 | useful scalar proof profile and live solver runner | [verification](verification/plan.md) |
| S1-10 | package effect and security analysis | [analysis](analysis/plan.md) |
| S1-11 | executable release evidence pipeline | [evidence](evidence/plan.md) |
| S1-12 | executable release guide and acceptance map | [docs](docs/plan.md) |

The combined native/LibTorch lane avoids conflicting binding lock and ABI edits. The runtime and ownership lanes are separate additive libraries. The worker transport starts independently from a frozen wire contract, with later integration explicitly assigned. The execution lane has a real S2 consumer: owned-input map/zip source lowering and structured performance analysis; it must not invent extra kernels to fill a chat.

## Complete release requirement ledger

“Open” means some required acceptance is still missing, including where a preview implementation already exists. All rows start open; plans earn no completion credit.

| ID | Required outcome | Delivery/dependencies | Closure evidence |
| --- | --- | --- | --- |
| R01 | Canonical syntax, nominal records/variants, exhaustive matching, Option/Result | S2a after S1 core and frozen consumers | Parser/formatter/API round-trip; qualified public identities; constructor/pattern checks; unreachable/nonexhaustive errors; interpreter/native parity |
| R02 | Generics with named constraints and usable standard data | S2b after R01 | Substitution/inference/arity/constraint failures across packages; lists/strings/checked indexing and error handling in runnable applications |
| R03 | Safe ordinary managed/owned memory, borrowing, region transfer, manual/auto and no_gc | S1 memory/ownership → S2b compiler → S3 notebook/foreign → S4 | Source and runtime invalid programs reject; lifetime/root/cleanup corpus; fault/cancel/unwind paths; leak-enabled external checks and independent audit |
| R04 | Modules, explicit public effects/capabilities, checked runtime contracts | Wave28 baseline + S1 analysis/notebook → S2b | Import provenance and transitive checks; dishonest/unknown declarations reject; enforceable capability checks; contract errors agree across execution paths |
| R05 | Native installed CLI and structured diagnostics/API | S1 core/docs → S2b command/package integration | Installed check/build/run/test/verify/emit/fmt/api/notebook command matrix, UTF-8 source locations, missing symbol vs dependency, stable structured outcomes |
| R06 | Reproducible packages and foreign declarations | S1 native/python/worker → S2b | Explicit compiler/runtime/artifact/environment pins, deterministic offline rebuild, drift/target rejection, bind check, no install from import |
| R07 | Explicit Python-only access | S1 python → S2b source → S3 package fixture | Real pinned Python-only package from .cg; identity/exception/lifetime failures; native tensor path never silently chooses Python |
| R08 | Direct typed LibTorch creation/inspection/math/inference/conversion | S1 native → S2b tensor types → S3 application | Real CPU inference/data CLI vs native reference; shape/dtype/device and release checks; no libpython; supported GPU only with actual device evidence |
| R09 | Native notebooks with persistent state and no replay of earlier side effects | S1 notebook/worker → S3 notebook | Contract parity; one selected cell effect once; explicit rerun once; uncertain outcome handling; restart/clean run, invalidation, stdout/errors/tables/plots and portable safe outputs |
| R10 | Rust native output and stated portable Python profile | S1 core → S2a/b → S3 conformance | Same outputs/errors/checked arithmetic/contracts for admitted features; explicit unsupported target errors; blinded edit/readability study in S4 |
| R11 | Bounded integer/Boolean/checked-arithmetic/array-bounds/annotated-loop proofs | S1 verification → S2 syntax/types → S3 verification | Source/core/SMT audit, bounds/invariant obligations, genuine pinned solver results/witnesses; timeout/unsupported stays unknown; ownership safety explained separately |
| R12 | Security and performance analysis | S1 analysis/execution → S2b source → S3 adaptation | Known positive/negative package corpus, bounded taint/unknown reporting; actual performance decisions/reasons and complete dispatch costs |
| R13 | Memory/correctness and runtime/resource acceptance | S1 evidence and owner runners → S4 | Full supported corpus/audit; real leak-enabled checks; native/managed/LibTorch/FFI and bridge calibrated comparisons with uncertainty and complete provenance |
| R14 | Measured LLM/human value and grammar decision | S1 evidence/docs → S4 | Independently reviewed 60–100 task initial holdout, two model families, repeated paired trials, real human comprehension and blinded generated-source tasks; full failure-cost accounting and confidence intervals |
| R15 | Release packaging, support matrix, documentation and final review | All rows above → S4 | Clean installation and documented examples; complete gate manifest; supported/unsupported routes; every release blocker explicitly resolved |

No unrestricted standard-library/registry/debugger/LSP expansion is silently added to R02/R06. Minimal collections, strings, errors and operations needed by the accepted release workflows are required; mature ecosystem breadth remains longer-term. Security analysis remains advisory; R04 runtime enforcement is separate.

## S2: shared compiler/interface integration (dependency gate)

Start only after S1 changes to exhaustive AST consumers have been integrated and checked. One core integration owner temporarily owns syntax/sema/package/compiler/emitters plus mechanical compile-and-reject adaptations in notebook/verifier/analyzers. No concurrent worker may write these consumer paths during this interface gate.

**S2a — language and type representation.** Specify and implement nominal variants, payload constructors, exhaustive matching, Option/Result and public qualified identity. Freeze generic declaration/constraint, collection/array and loop-contract syntax before downstream work. Include explicit diagnostic treatment of unsupported constructs in every consumer; do not hide missing semantics under wildcard matches. Deliver parser, typed representation, formatter/API and interpreter/native tests. Generic/type shapes must be accepted by a documented compatibility commit before consumer chats restart.

**S2b — complete source semantics and command surface.** Implement generics/named constraints, supported standard data/checked indexing, ordinary managed/owned aggregates and function lifetimes via the S1 ownership/runtime APIs. Integrate explicit capabilities and typed foreign/tensor calls, pinned package environments and reviewed native/Python routes. Expose bounded owned-input map/zip with S1 execution policies and structured performance reports; auto remains sequential without valid evidence. Add test discovery/properties convention and installed test/verify/notebook/bind commands. Complete Rust output and documented portable Python semantics. Each subrequirement above must have source-level acceptance; a Rust library API alone is insufficient.

### S2b finite implementation checkpoints

The reviewed S2a work exposed overlapping missing representations for generics/data, ownership and typed foreign values. Keep one core writer until each compatibility checkpoint is reviewed. The accepted bounded delivery is [S2b foundation](s2b-foundation/plan.md), covering executable generic functions/records/variants, real named-constraint checking, List/Array/String/indexing and portable value-target semantics. Its proposed remaining envelopes are not source implementation evidence. The next executable plan is [ordinary function memory](s2b-memory/plan.md), with one core/runtime owner and no proposal-only stop.

The following requirements remain mandatory inside S2b after foundation; none is reassigned to a vague later wave:

| Checkpoint | Required source outcome | Requirements |
| --- | --- | --- |
| Foundation | Generic substitution/inference/constraints, usable standard data and checked indexing across packages and admitted targets | R02/R10 |
| Ordinary function memory | Managed/owned aggregates, borrowing/projections, region provenance, transfer, auto/manual/no_gc and failure/cancel cleanup via reviewed S1 APIs | R03/R04 |
| Typed package/foreign/execution integration | Authenticated effects/grants/pins, ordinary native/Python/tensor values and calls, explicit routes; owned map/zip and structured decisions with safe Auto fallback | R04/R06/R07/R08/R12 |
| Command/target closure | Installed test/verify/notebook/bind and property conventions, complete admitted Rust support and stated Python profile, source/target parity | R05/R10 |

A checkpoint may span several coherent commits. Additional parallel adapters are dispatched only for demonstrated missing library work against reviewed immutable envelopes. The S3 consumer fan-out follows the complete S2 interface gate.

S2 integration may split into commits, but the R01–R08/R10/R12 acceptance checklist is fixed. A failed check creates a linked repair item, not a new vague “wave”.

## S3: independent consumers and representative applications

After the reviewed S2 interface commit, dispatch independent work again:

| Package | Owner/path allocation | Depends on | Required result |
| --- | --- | --- | --- |
| S3-N | all notebook crates | S1 notebook/worker, S2 | Execution-once stateful effect cells, compiler type/profile adoption, safe cancellation/unknown effects, memory/foreign lifetimes and portable rich outputs |
| S3-V | verifier crates | S1 verification, S2 invariant/array syntax | Array bounds and annotated-loop obligations, sound bounded semantics, new-type rejection/admission, live solver acceptance |
| S3-A | effect-index/security | S1 analysis, S2 | New type/foreign/capability propagation with scoped rule coverage and source-located outcomes |
| S3-C | core/compiler/emitters/package | S2 | Installed command closure, cross-target conformance and reference .cg applications using reviewed library APIs |
| S3-M | runtime/memory-conformance/ownership | S1 libraries, S2 memory lowering | Full supported memory/foreign/cancel corpus and audit fixes; source-owned repairs assigned to S3-C |
| S3-F | native/Python/worker owners | S1 engines, S2 foreign contract | Real pinned package fixtures and source-call/cleanup/offline packaging acceptance |
| S3-D | docs/first-release, tools/docs-check | S2/S3 feature commits | Every documented release example runs with supported outputs/errors and portable installation |
| S3-E | bench | reviewed S3 feature set | Immutable preregistered candidate, quiet-host runtime study and genuine externally reviewed model/human handoffs |

### Ten-ecosystem plan is explicit

The interoperability documents describe a broader package-access rollout. Track it separately, never call it complete because native/Python preview calls work. These concrete adapter tasks become ready after the native/worker and S2 foreign/lock contracts freeze:

- E1 C zlib variable-length round trip; E2 C++ fmt facade with exception containment; E3 Rust regex facade with panic containment.
- E4 Python dateutil using the pinned explicit Python route.
- E5/E6 one Node engine for JavaScript lodash and compiled TypeScript date-fns, Promise/error mapping and reviewed exports.
- E7 JVM/JNI Commons Lang with classpath, attachment and reference lifetime checks.
- E8 .NET hostfxr/Newtonsoft.Json with runtimeconfig, assembly and GC-handle checks.
- E9 PHP/Composer ramsey/uuid through the persistent worker.
- E10 Go x/text through reviewed c-shared or explicitly selected supported worker route.

After SDK freeze these can fan out into **eight further adapter chats**: native C/C++/Rust fixtures together, Python, Node JS/TS together, JVM, .NET, PHP, Go, and direct tensor reference integration. They are not launched now against incompatible unpublished APIs. Report actual passing representative routes as N/10, including source entry, wrong-version/effect/lifetime failures and offline artifacts. R06–R08 require their stated first-release routes; claiming all ten ecosystems additionally requires E1–E10. No expansion to arbitrary universal package parity is implied.

General GPU kernel generation, distributed execution, browser-only replacement, Jupyter integration, Arrow/DLPack zero-copy, debugger/LSP/registry maturity and OCaml-scale ecosystem breadth retain their design's later-platform status. Available supported GPU tensor devices need actual tests; absence cannot be disguised as CPU evidence.

## S4: external acceptance and release decision

The release gate is a conjunction of required capabilities and evidence, not an average score. Use the executable gate report from S1 evidence. Required statuses: passed, failed, blocked, unrun; only passed closes a row, with immutable evidence and review provenance.

1. Freeze a candidate commit/toolchain/environment/task set. Run integrated tests, command/profile examples, native/Rust/Python parity, full memory and foreign negative corpus, independent safety review.
2. Run leak-enabled memory/FFI checks on a capable host. This host has documented ptrace restrictions; ASan with detect_leaks=0 is not leak evidence.
3. Run actual trusted/pinned Z3/CVC5 cases; finite enumeration/mock solvers are distinct. Solvers previously absent from PATH.
4. Run positive/adversarial scoring isolation on a host with delegated cgroup CPU/memory/pids and required isolation. No unsupported-host pass.
5. Run runtime measurements on quiet calibrated hosts, charge startup/load/copy/transfer/synchronization and report cold/warm distributions and intervals. Proposed gates from validation plan remain: native ≤1.25× Rust time/≤1.5× RSS; managed ≥70% OCaml throughput/≤2× RSS; direct inference ≥90% throughput and ≤1.15× p95 vs each applicable native/Python baseline; native/Python boundary overhead ≤1.25× after calibration. Failed targets remain failed.
6. Conduct genuine independently reviewed paired LLM/human studies: at least two model families, repeated randomized pairs, locked budgets/tools/tasks and real human participants. Completion noninferiority and ≥20% lower paired total tokens/time require their specified confidence bounds; failure cost is charged fully. No synthetic traces or agent self-ratings substitute.
7. Run clean installation and documentation; close all R rows with linked evidence, record limitations/platform support and obtain the user's release/merge decision. No automatic main merge or publication.

External capacity/runtime/package availability, independent review and human/model studies are unresolved dependencies. Workers may build executable handoffs now; they cannot fabricate these outcomes. No time estimate assumes these dependencies away.

## Progress and dispatch accounting

See [dispatch.md](dispatch.md) for actual chat creation outcomes. Stage 1 is “planned/queued/running/delivered/reviewed”, distinct from R-row closure. The coordinator records each delivered commit, checks ownership, integrates on a review branch and updates the gate ledger. Workers stop after their bounded delivery. Starting a new dependency stage requires its reviewed contract/base, not merely a “done” message.

The user subsequently authorized a 20-minute heartbeat to review completed work and launch dependency-ready follow-on chats. Coordinator dispatch may proceed after review gates; workers still commit one bounded task and stop. No merge to main, publication, external paid studies or worker-to-worker messaging is authorized.

## Owned Result review update

The recoverable allocation delivery is staged and existing integration checks pass, but source compatibility and runner output bounds fail review. See [review and repair plans](s2b-owned-result-review/README.md). R03/R04 and the full release requirement ledger remain open; implementation and acceptance are tracked separately.

## Owned Result repaired acceptance

Both repairs and the bounded Result/no_gc delivery are accepted with1237Rust/2928 current product checks, exact historical-negative reconciliation and evidence. The next sole shared producer is [default ordinary managed heap](s2b-default-managed/plan.md). R03/R04/full memory remain open; all release rows stay required.

### Managed verification infrastructure update

At the 2026-10-24 07:50 UTC heartbeat, default managed implementation remains running and unreviewed. A temporary-directory quota interruption was relieved by clearing an inactive Rust incremental cache; this is infrastructure recovery only, with no new acceptance result. R03/R04 and every remaining release requirement retain their current open status. See dispatch.md for the precise cleanup and worker identity.

### Managed native boundary acceptance remains open

At 2026-10-24 08:50 UTC the worker reports native compilation of the required 65,536-element fixture exceeded the runner 4 GiB compiler bound, despite interpreter checking/execution passing. The existing producer is repairing emitted model size and retaining the failed evidence. No native boundary pass or managed delivery has been accepted; the full release ledger is unchanged.

## Managed delivery review: repairs required

Default managed bc4ba95 is staged for repair, not accepted. Independent review reproduced source-provenance mismatch, nominal AllocationError compatibility regression and representation-dependent Result equality; required native fault/budget and rejection evidence is incomplete. See [review and three repair plans](s2b-default-managed-review/README.md). Existing workspace/regression successes do not close these failures. R03/R04 and every remaining release requirement stay open; the next frontier is bounded repairs, not S3 consumers.

### Two managed repairs accepted; combined gate pending

Equality and evidence repairs are accepted as bounded deliveries (c287985/1b922e2). The core source-authentication/nominal repair remains running. The stronger matrix retains8 known entry-span failures; core integration, the unchanged-assertion layout-test call-site reconciliation and a full combined rerun remain required. Full managed/R03/R04 acceptance and all release requirements stay open.

### Combined managed checkpoint accepted; remaining memory contract ready

See [combined acceptance](s2b-default-managed-review/evidence/combined-accepted/acceptance.json). All three repairs and coordinator caller/harness fixes are reviewed with unchanged frozen compiler artifacts. Original failed attempts and superseded historical negative are retained. The next connected memory plan freezes region brands, graph copies, owner-preserving failed promotion, aggregate ownership and pre-Session fallback. Only this shared producer is dependency-ready; no S3 fanout. All R01–R15 rows remain required.

### Ordinary-memory delivery incomplete; bounded repairs required

[Review and four repair plans](s2b-ordinary-memory-review/README.md) record8 reproduced source/runtime/API defects in a3b98d7, staged32032fd solely as a repair base. Passing1327 workspace tests does not accept it. Full aggregate ownership, source copy_owned/promote, authenticated v3 summaries, static dependency-file attribution and required fault/cleanup/parity remain open. R01–R15 are preserved; no S3 consumers or release.

### Ordinary API repair accepted; combined memory gate pending

OM08 public API projection is accepted as bounded repair64a71e7 with independent175Rust/84product checks and reviewed historical evidence. Admission/core review and runtime/evidence deliveries are still required. All broader memory and R01–R15 requirements remain open and unchanged.
