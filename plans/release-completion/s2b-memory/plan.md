# S2b ordinary function memory — executable source checkpoint

Status: dependency-ready after reviewed foundation acceptance. One sole core/runtime owner. Requirement focus R03/R04; preserves all remaining R01–R15 requirements. This completes the ordinary-memory source checkpoint, not all S2b or external memory acceptance. Several coherent implementation commits are allowed; do not stop with only a proposal, hand-authored IR example or library-only adapter.

## Read first and ownership

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/{README,REVIEW}.md, plans/release-completion/{README,contracts,worker-rules}.md, s2b-foundation/plan.md, s2b-review/README.md, crates/cgx2-syntax/S2B-FOUNDATION.md, crates/cgx2-ownership/ADAPTER.md, runtime README, and memory-conformance/release-memory.md. MegaMemory overview/query/record and Serena initial instructions required.

Full implementation ownership:
- crates/cgx2-syntax/**
- crates/cgx2-sema/**
- crates/cgx2-package/**
- crates/cgx2-api-index/**
- crates/cgx2-cli/**
- crates/cgx2c/**
- crates/cgx2-emit-rust/**
- crates/cgx2-emit-python/**
- crates/cgx2-runtime/**
- crates/cgx2-ownership/**
- crates/cgx2-memory-conformance/**

Mechanical exhaustive compatibility and conservative admission only:
- crates/cgx2-notebook-format/**, crates/cgx2-notebook-kernel/**, crates/cgx2-notebook-session/**, crates/cgx2-notebook-cli/**, crates/cgx2-notebook-ui/**
- crates/cgx2-verify-core/**, crates/cgx2-verify-cli/**, crates/cgx2-effect-index/**, crates/cgx2-security/**

Owned contract/evidence: crates/cgx2-syntax/S2B-MEMORY.md and crates/cgx2-syntax/s2b-memory-evidence/**. Other tests under owned crates. Root Cargo.toml/Cargo.lock, shared docs/plans/ledgers, bench/tools and all foreign/bind/ffi/tensor/worker/execution/conformance adapter crates are read-only. Existing local path dependencies may be added in owned manifests; report exact root-lock reconciliation, excluding generated root lock edits from commits. No third-party dependency/package installation.

Work only in the attached worktree on a riley/ branch. No delegation, new chats, shared target directories, main merge, publication or automatic follow-on task. This sole writer may update shared core representations and mechanically adapt all consumers in scope. Preserve existing public runtime/ownership APIs with additive/versioned extensions; do not silently reinterpret their contracts. No downstream consumer is authorized to implement against unreviewed shapes.

## Coherent deliverable

Implement ordinary function parameters/results/locals, control flow, aggregates, generics and package calls with owned moves/loans plus default managed graph allocation, regions, transfer and no_gc, connected to actual runtime storage and native execution. Preserve foundation source behavior and all repaired generic/data/target cases. Document precise provisional source grammar, semantics, typed summaries, runtime layout and target admission in the owned contract before coding their implementation; then deliver working source and evidence for those contracts.

### 1. Owned function analysis and authenticated summaries

Lower real parsed/resolved ordinary functions into cgx2_ownership::v1::analyze_function(function, summaries, source, limits), admitting only complete AnalysisReport::is_accepted(). Use compiler-resolved local/place/layout/region/loan IDs and original UTF-8 source spans. Parameters/results express move, shared/exclusive borrow, projection and provenance; returning a loan requires an eligible borrowed input, never a local/consumed owner. Cover calls, nested scopes, branches, loops, match/early returns, recursive SCCs and generic specialization. Map copied parameters and ownership-bearing Result branches explicitly: v1 has no Copy parameter mode and only one result mode. Support sound partial moves and branch joins; preserve failed-transfer ownership. Check callees before publishing authenticated summaries; bind signatures/body/source hashes, modes, layouts, effects/capabilities and transitive imported dependencies. Verify unchanged package source before use.

Preserve v1 defaults (blocks 256, operations 4096, locals 256, summaries 256, projection depth 16, name bytes 256, steps 10000, state entries 65536, source bytes 4 MiB) unless an explicit bounded versioned extension is needed and documented. Do not treat the v1 unique Region storage policy as proof for freely aliased managed objects. Its unsupported aliasing/reference fields, dynamic-index moves, reborrow and graph cases require sound additive analysis/lowering or clear source diagnostics; a safety rejection cannot be reported as implementation of a required source capability.

### 2. Actual default managed storage and layout

Ordinary heap allocations use the default managed region; implement safe ordinary records/variants/List/Array/String storage and managed references with roots at allocating safepoints, not a hidden unaccounted host side table. Default region behavior stays auto, trigger 1 MiB, limit 64 MiB; managed handles remain valid across collection. Root params, locals, result transfers and temporarily evaluated children before subsequent allocation. Preserve source evaluation order and single evaluation. Shared managed graph aliases/cycles must obey runtime tracing and region/session identity; do not mislabel a unique-buffer-only implementation as default managed graphs.

Use Session::try_new, try_no_gc, scoped_roots and ScopedRoots::{root,cleanup,finish}; cleanup must preserve the original success/error/cancellation and separately report cleanup failures. Runtime object payloads are currently zero-initialized with read_bytes and reference/set_reference, but no public checked payload-write API; add a checked additive path for typed payload initialization/update when needed. Validate layout, payload bounds and reference slots without exposing raw pointers. Existing max 256 reference slots/object and max 1 MiB payload must coexist with admitted collections up to 65536 elements: implement and document bounded chunk/layout handling or an explicit compatible representation, not a silent reduction of prior valid data support. Account all CGx2 storage and use checked allocation arithmetic.

### 3. Regions, transfer, recoverable failure and no_gc

Named auto/manual regions retain explicit limits and closure rules. Auto collection occurs only at documented safepoints; manual collection only on explicit collect. Closing requires no escaping live references/loans; region close differs from a collection. Cross-region movement/copy and owned↔managed conversion are explicit with specified copy/allocation/invalidation behavior. Managed-to-owned may use checked unique transfer or explicit copy; it cannot steal a shared object.

Allocation/budget/region/transfer failures have source-level recoverable Result semantics with exact owner state on success/failure. Do not consume and lose an owned input on a failed promotion/copy. Define payload/provenance of error continuations so source checking and runtime agree; no rollback promise for earlier successful mutations. Static impossible cases reject where proved, runtime checks enforce the rest.

A no_gc block suppresses collection throughout the current single-threaded CGx2 session, across all regions. Charge every active nested budget before commitment, including runtime bookkeeping covered by the runtime contract. Exceeding a budget returns AllocationError without that allocation or collection; explicit collect rejects inside the block, including hidden/transitive collecting callees through generic and package calls. Releasing guard/root scopes must not collect a manual region. Foreign allocation remains foreign and cannot satisfy CGx2 accounting. No automatic parallel managed-session sharing.

### 4. Cleanup, runtime reporting and targets

Generate deterministic cleanup for owned values and roots on normal exit, return, branch failure, checked arithmetic/contract failure, recoverable allocation errors, injected host-reserve failures and supported cooperative cancellation/unwind paths. Preserve session/storage identity and root registrations; do not introduce infallible allocation into required fallible runtime entry points. Define which external termination paths are outside lexical guarantees without weakening supported paths.

check/run/build/Rust output and package execution must share source admission and memory semantics. Actually execute native debug/release; standalone emitted Rust must include/use the authenticated required runtime rather than a host-only simulation. Preserve legacy memory-script commands and reporting; extend structured reports additively for ordinary functions with live/peak/committed bytes, failures, collection counts and cleanup/root evidence. Library-only counters are insufficient unless generated from real .cg execution.

Python retains the documented portable application-value profile and explicit rejection of regions/no_gc/owned borrowing or other unsupported observables before artifact replacement. Do not silently substitute Python GC for explicit CGx2 guarantees. Preserve all prior admitted portable foundation examples. Notebook/verifier/analyzers must conservatively reject unsupported new children/modes before old walkers; no false proof or clean report.

## Required acceptance

Commit runnable .cg/package fixtures, expected values/errors/source spans, target support matrix and a reproducible bounded product runner. At minimum:
- Ordinary functions using owned aggregate parameters/results, shared/exclusive loans, projections, nested calls, recursion/loops/matching and generic instantiations; valid paths execute, moves/overlapping loans/escaping loans/invalid closes and unauthenticated summaries reject.
- Package exported and private bodies with transitive lifetime/effect provenance; stale source/signature/layout summaries reject. No generated specialization changes public source privacy.
- Default managed records/variants/collections with shared aliases and cycles, roots surviving collections at nested call/argument safepoints, unreachable graph reclamation and isolated regions. Include collections spanning more than 256 references and nested generic data.
- Auto/manual/no_gc across zero/exact/over-budget boundaries, nested guards and regions, explicit collect rejection, recoverable failures and correct owner usability after failed transfer. Verify zero session collections under no_gc on success/error paths and no manual exit collection.
- Source-driven fault injection, early-return/contract/arithmetic/allocation/cancel cleanup, repeated entry/exit, no leaked roots/owners and preserved original errors. Reuse runtime conformance/state-machine coverage and extend for new APIs/layouts; classify synthetic fault evidence accurately.
- Actual interpreter/native debug/native release/standalone Rust values/errors/report invariants; exact supported Python parity and artifact-preserving unsupported rejection. Execute outside repository-relative assumptions.
- Full all-feature offline locked workspace tests, all-target/all-feature Clippy -D warnings, fmt and git diff checks; foundation 159 and all owned affected source/package/API regression runners remain passing.
- Maintain operator-ready leak-enabled/independent memory safety audit evidence commands. If this host cannot run LeakSanitizer, retain blocked status and exact prerequisite; do not claim leak acceptance from disabled-leak tests. No package installation or host security changes.

Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=$PWD/target/release-s2b-memory, TMPDIR=$PWD/target/tmp-s2b-memory; create private directories. LIBRARY_PATH=/tmp/cgx2-wave22-lib if needed. No performance measurements or external APIs/studies/contacting people. Record failing checks honestly; no feature success inferred from documentation alone.

## Completion and remaining requirements

Complete this ordinary-memory source checkpoint, commit code/contracts/evidence and stop for coordinator review. Report exact commits/files, runtime and compiler APIs/defaults, actual source/target counts, dependency reconciliation and remaining limitations. If a concrete design blocker prevents an acceptance row, report it explicitly as incomplete with the smallest implementable resolution; do not silently defer or rename the requirement.

Typed foreign/package/tensor/owned map-zip execution, remaining installed commands/target closure, S3 consumers/applications, external leak/solver/isolation/performance/audit and paired human/model gates remain mandatory separate checkpoints. This memory implementation does not close them or establish full S2b/release readiness.
