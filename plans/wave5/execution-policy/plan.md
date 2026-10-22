# CGx2 Correctness-First Execution Policy Plan

> Implement this plan in your attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Establish a measurable, safe decision boundary for sequential CPU, parallel CPU, and future GPU execution, while implementing one real pure CPU parallel primitive. Do not claim arbitrary `.cg` code is automatically optimized.

**Architecture:** Add `cgx2-execution-policy` as a source-independent library. A typed `Workload` describes element count, bytes, pure operation kind, associativity/order constraints, CGx2 memory activity, effect flags, and whether a `no_gc` session is active. `Capabilities` describe available backends and measured overhead/throughput ranges. `choose(policy, workload, capabilities) -> Result<Decision, PlanError>` returns a backend plus a reason and estimated end-to-end cost. `Auto` chooses only proven-safe supported paths and defaults to sequential CPU when measurements or capabilities are missing. Implement one deterministic `i64` elementwise map using `std::thread::scope` so scalar and parallel CPU results can be compared on real data. GPU remains a capability interface that returns unsupported absent an actual backend; a mock may test decisions but must be labeled mock.

**Tech stack:** Rust 1.98, standard library only, no network dependencies or `unsafe`.

**Specification:** `docs/language-design.md` “Automatic execution choices”, `docs/validation-plan.md` execution-choice validation, `plans/wave5/README.md`.

## Owned paths and fixed boundaries

- Own only new `crates/cgx2-execution-policy/**` and this plan. Do not edit parser, CLI, runtime, Python bridge, benchmark harness, root manifests/lock, or shared docs.
- Public policy values: `Auto` (default), `SequentialCpu`, `ParallelCpu`, and `Gpu`. An explicit unsupported backend returns a structured error; `Auto` falls back to sequential. The decision must expose `selected`, `reason`, and calibrated or uncalibrated cost status. Never claim a global optimum.
- Parallel is eligible only for a pure independent operation with a reviewed elementwise contract, no shared mutable alias, no observable order dependence, and no cross-thread use of the existing single-threaded managed `Session`. `no_gc` must not be bypassed by workers that share CGx2 memory. Foreign PyTorch GPU memory is not charged to CGx2 region budgets.
- The example map uses checked `i64` arithmetic with deterministic output/error location in source order. If any worker would overflow, return the same first offending element/error as sequential execution. Bound worker count and chunk size; avoid spawning threads for tiny workloads. `Gpu` cannot execute without a real backend and capability probe, even if a cost estimate says it would be faster.
- Costs include launch/setup, CPU work, host/device transfer, synchronization, and expected memory pressure. Missing or stale measurements mean `Auto` falls back to the correct sequential path. Float reductions need an explicit numerical contract and are out of scope.

## Review focus

1. Scalar and parallel elementwise map agree on values and first checked-overflow index across sizes, worker counts, and boundary inputs.
2. Effectful, order-sensitive, aliased, managed-session, and `no_gc` workloads cannot be parallelized by this prototype. Explicit `ParallelCpu`/`Gpu` requests fail clearly if unsafe/unsupported.
3. `Auto` picks sequential on missing measurements and on small workloads where overhead dominates; picks parallel only when a measured conservative cost interval shows benefit.
4. GPU is never selected without a real executable backend. Cost-model mock tests cannot be presented as GPU execution.
5. A benchmark helper records raw scalar/parallel samples and overhead under varied input sizes, but timing noise cannot invalidate correctness assertions or establish a native-language speed claim.

## Tasks

### 1. Policy and capability contract

- [ ] Add red tests for policy defaults, missing data fallback, explicit unsupported requests, effect/memory exclusion, and estimated costs including transfer/synchronization.
- [ ] Implement public policy/decision/error structs with stable reason codes and deterministic selection. Document parameter names and defaults.
- [ ] Add test-only mock GPU capability but keep production `Gpu` unavailable until a backend exists.

### 2. Real CPU primitive and measurement

- [ ] Add red equivalence tests for checked `i64` elementwise map on sequential and scoped-thread implementations, including empty/singleton/large arrays and first-overflow index.
- [ ] Implement bounded CPU parallel path and an opt-in calibration helper that records raw samples across multiple sizes, warmups, worker counts, and machine provenance. It may propose a crossover for this one operation only.
- [ ] Show in tests that the planner does not choose a parallel path merely because more cores exist; it needs capability and measured benefit.

### 3. Verification and handoff

- [ ] Run `cargo test -p cgx2-execution-policy --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-execution-policy --all-targets --offline -- -D warnings`.
- [ ] Update README with a clear example, raw calibration command, supported primitive, unsupported GPU status, and required future typed-IR/backend integration. Commit only owned paths.

**Acceptance:** A real pure CPU operation can use a correct parallel implementation under a conservative measured policy. This is a foundation for future language-wide analysis and GPU dispatch, not an automatic GPU compiler.
