# S2b foundation review and repair frontier

2026-10-24 01:10 UTC heartbeat. Final delivery 414c71f0cae8c66c822f0b6fb2be83219c1f4c95 is staged as b0900dd20746a6a80ca83bfb02f11f1de2a15230, **repair-required**. The earlier unamended 7fa8ecc was not integrated. All 75 paths fall within foundation ownership; no root manifest/lock/dependency changes. Worker .serena remains untracked and excluded.

## Evidence reviewed

Worker logs independently match all four recorded SHA256 values: workspace 1092 passed, 0 failed, 6 ignored across 243 suites; Clippy/default build/product logs verified. Recorded 159 product checks pass. Implementation hash 553a62230e821c0b593dbdaab6ef8b1fe0f83ce248f7f10e157d5778a77ba1b4 independently recomputed by review. Post-suite trailing-blank normalization is explicitly recorded; product/build/Clippy/fmt rerun afterward. Coordinator staged all-target/all-feature offline locked cargo check passes. These checks do not override reproduced failures.

## Confirmed blockers

| Repair | Finding | Required result |
| --- | --- | --- |
| semantic-resolution | Intrinsics intercept existing record constructors; binder-shadowed constructor passes checking without complete type | Correct nominal resolution and complete source checking |
| package-binders | Exported generic fields/payloads qualify binder T as same-named local record T | Binder-aware exported types and imported source/target parity |
| emitted-targets | Local-only Option specialization missing from Rust declarations; unsupported string escape panics Rust and is interpreted differently by Python | Complete concrete declaration discovery and safe consistent string validation/quoting |
| interpreter-output | Package run writes 1,242,001 bytes where Python enforces 1 MiB cap | Bounded finalization and rendering on module/package paths |
| api-visibility | Public API schema7 includes private owner/callee instantiations | Public-only metadata projection with internal evidence preserved |

Repro sources and captured outputs are in evidence/. Native failure was reproduced with rustc --edition=2021; no obsolete-edition errors are counted. Repro subprocesses used 512MiB address-space and 15 CPU-second limits (type-expansion 10 CPU seconds), 20-second wall timeout.

Contract-only data test passed source/run/emission/Python and is excluded as a blocker.14-level type-expansion rejects CGT030; early expansion allocation concern is investigation-only, not a claimed failure. No uncontrolled stress run.

Five disjoint repair plans preserve existing shared shapes. Emitters share a string-parity defect and are one coherent lane. Full combined workspace tests/Clippy/format/build,159-product runner, all original failure repros, docs/bench/readiness and affected package/API checks are required after repair integration. Do not advance ordinary memory, foreign/execution/commands or S3 until the foundation is accepted.

## Release gates

R02/R10 foundation acceptance is failed pending repair. All remaining S2b memory/foreign/execution/installed-command requirements and all 15 master R rows remain mandatory. Trusted live solvers, leak-enabled memory/FFI, scored isolation, quiet-host performance, independent review and genuine paired human/model studies remain open. No release percentage, S2b completion, main merge or publication.

## Five repairs reviewed and integrated — 2026-10-24 01:30 UTC

All five owned patches accepted as bounded repairs; foundation remains repair-required. Original/integrated commits: semantic 559c390/5c5557e, package 2412974/546a063, emitted c0dabf5/323574e, output 65a9448/5dfb0c7, API 762d9cc/507feaf. Source ownership inspected; no shared representation, dependency or root-lock change. Three independent read-only reviewers accepted semantic/package, emitters and output; coordinator reviewed API projection and its tests.

Combined workspace: **1126 passed, 0 failed, 6 ignored**, 248 suites. All-target/all-feature Clippy -D warnings, fmt and default binaries pass. Foundation product runner: **159 passed** after updating its obsolete expectation of private call metadata to check both private-call omission and complete public-call retention. Original runner failure retained in log, not relabeled as a product pass. Evidence: evidence/round2/integration.json and foundation-product.json.

Coordinator rechecked imported binder returns 42, binder-shadowed constructor rejects CGT002, package output rejects CGR010 with no stdout and private API emits no instantiations. New emitter regression tests actually compile/execute multiple local Option/Result specializations and validate escaped literals.

### Remaining concrete blockers

1. Native finite emitter still dispatches record constructors by intrinsic spelling: original length record checks/interprets 42 but emitted Rust fails E0277/E0308. Semantic worker retained 33 native failures across original plus 10 names; the concurrent target repair addressed different emitter defects.
2. Named standalone concrete types acquire doubled prefixes during lowering: public_api.public_api.Value and out.main.out.main.Long. Source check succeeds but execution/emission rejects.
3. Valid public generic calls with caller-private type arguments fail internal specialization rechecking with CGT022; genuine source public-signature privacy must still reject.

The last two share the source-specialization owner; the first belongs to native-constructors. Both bounded plans preserve shared layouts and use the integrated base. Actual dispatch outcomes are in ../dispatch.md and ../thread-state.json. No consumer feature fan-out or additional busywork; remaining S2b/R02/R10 and all external gates stay open. Full final acceptance including docs/bench/readiness still follows successful defect closure.

## Foundation accepted after follow-up repairs — 2026-10-24 01:50 UTC

Source-specialization 74d1116 → fb7649e and native-constructors ce7880d → c233dd6 are independently reviewed and integrated. Changes respect assigned paths and preserve original-source privacy/effect/trust validation; concrete implementation declarations are internal. No root manifest/lock/dependency reconciliation needed.

Combined evidence on c233dd63742ccd2b36cc5870881961b54dd71c84:
- Workspace **1141 passed, 0 failed, 6 ignored** across 251 suites; all-target/all-feature Clippy -D warnings, fmt, default workspace binaries pass.
- Product suites: foundation 159; semantic resolution 278; source specialization 207; public API visibility 118; package binders 88; output bounds 31; emitters 104. **All pass.** Actual interpreted/native debug/native release/standalone Rust/Python/package executions include original and expanded regressions; rejected artifacts remain preserved.
- Benchmark harness 166/166 and docs harness 20/20 pass. Executable docs 39 pass, 0 fail, 6 blocked, 7 future; acceptance_complete=false.
- Public syntax readiness source-tree pin refreshed from 15078906 to 9200f0f after stale-pin failure; compiler/Cargo executable pins and Cargo.lock hash unchanged. All 6 reference checks pass; model traces 0, human review unrun, scored isolation unsupported.

Copied immutable reports are evidence/accepted-*.json. accepted-integration.json records actual integrated source commit, counts, logs/hashes, preliminary invocation/configuration issues and their successful reruns. Earlier failed historical reports remain untouched.

**Foundation accepted as a bounded source capability checkpoint.** R02 generic/data implementation is reviewed; broader R10 portability/readability, ordinary memory, foreign/package/execution, commands, S3 integrations and external release evidence remain mandatory and open. No whole-S2b/release completion.

Next dependency-ready deliverable is [ordinary function memory](../s2b-memory/plan.md): one core/runtime owner implements actual owned loans and default managed graphs, regions, recoverable errors and session-wide no_gc in ordinary functions/packages. No other implementation lane is ready against reviewed memory interfaces. This is executable source/runtime work, not a proposal-only handoff.
