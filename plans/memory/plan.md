# CGx2 Memory Runtime Kernel Implementation Plan

> **For agentic workers:** Use the executing-plans skill task by task. This chat owns one bounded deliverable; do not spawn another agent, edit shared files, merge, or start a second assignment. Stop for user review after verification.

**Goal:** Build and test an isolated, memory-safe Rust kernel for CGx2 collection regions, rooted byte objects, explicit collection, and budgeted `no_gc`.

**Architecture:** A single-threaded `Session` owns regions and nonmoving object slots. Opaque generation-tagged handles remain stable until the object is reclaimed; roots and same-region references form a graph for mark/sweep. RAII `NoGcGuard` borrows the session mutably, suppresses collection session-wide, and charges every allocation to each active budget.

**Tech Stack:** Rust stable 1.98.1, standard library only; no `unsafe` in this first slice.

**Spec:** `docs/language-design.md` memory section and `docs/validation-plan.md` hard memory gates.

## Global constraints

- Own only `crates/cgx2-runtime/**` and `plans/memory/plan.md`; no shared manifests or compiler checker edits.
- This slice stores byte buffers and explicit same-region object references. It does not implement the later compiler's owned/borrowed type rules or arbitrary traced CGx2 records.
- Default session region is auto with a 64 MiB committed-byte limit and 1 MiB trigger; callers can create named auto/manual regions with explicit limits.
- Charge byte-buffer capacity plus fixed slot metadata before committing an allocation. Use fallible reservation; host allocation failure is a distinct error.
- Manual regions never collect on allocation or guard exit. Region close releases all remaining storage only if no roots/escaping handles remain.
- A no-GC guard suppresses all CGx2 collection in the session. Nested guards charge all active budgets. No collection or hidden budget increase on failure.

## Public contract

Create `crates/cgx2-runtime/Cargo.toml`, `src/lib.rs`, `src/region.rs`, `src/heap.rs`, `src/budget.rs`, `tests/regions.rs`, and `README.md`.

Expose `Session`, opaque `RegionId` and `Handle` with slot generation, `CollectionMode::{Auto { trigger_bytes: usize }, Manual}`, `AllocationError::{BudgetExceeded, RegionLimit, HostAllocation}`, and `MemoryError::{InvalidHandle, CrossRegionEdge, RegionHasRoots, CollectionForbidden}`. Public methods: `Session::new()`, `create_region(mode, limit_bytes)`, `alloc_bytes(region, len)`, `root(handle)`, `unroot(handle)`, `link(parent, child)`, `read_bytes(handle)`, `collect(region)`, `close_region(region)`, `stats(region)`, and `no_gc(budget_bytes) -> NoGcGuard`. The guard provides allocation, region creation, rooting/linking, and nested guard operations through its exclusive session borrow; a collection attempt returns `CollectionForbidden`. Document exact return types in the crate README and keep the API coherent with Rust borrowing.

`RegionStats` includes committed/live/peak bytes, allocation failures, collection count, and total collected bytes. It excludes foreign/Python memory. A handle to a reclaimed slot returns `InvalidHandle`, never a different object.

## Review focus

1. Allocation failure inside a guard must leave no half-created object and must preserve earlier writes.
2. An auto region reaching its trigger inside a guard must defer collection, not collect.
3. A manual region must retain unreachable objects until explicit `collect` or safe region close.
4. A stale generation-tagged handle must never address a reused slot.
5. A cross-region edge or closing a rooted region must return a clear error.

## Tasks

### T1: Region ledger, opaque handles, and allocation

- depends_on: []
- owns: [`crates/cgx2-runtime/**`]
- non_goals: [compiler ownership checks, foreign allocation accounting]
- consumes: [memory semantics in the shared spec]
- produces: [`Session`, region/handle/error/stats API]
- estimate: L
- coordination_risk: low
- status: complete

- [x] Add failing tests for default region values, manual/auto creation, exact byte charging, region-limit error without allocation, and stale handle after slot reuse.
- [x] Run `cargo test --manifest-path crates/cgx2-runtime/Cargo.toml --offline` and confirm the new tests fail.
- [x] Implement slots, generation validation, fallible byte reservation, charge accounting, and byte access without unsafe code.
- [x] Run the crate tests and verify failure leaves stats and existing payloads consistent.

### T2: Roots, graph tracing, and collection

- depends_on: [T1]
- owns: [`crates/cgx2-runtime/**`]
- non_goals: [arbitrary typed object tracing]
- consumes: [opaque handles and region ledger]
- produces: [`root`, `unroot`, `link`, `collect`, `close_region`]
- estimate: L
- coordination_risk: low
- status: complete

- [x] Add failing tests for reachable cycle retention, unreachable cycle reclamation, manual-only collection, auto-trigger collection, cross-region edge rejection, and rooted-region close refusal.
- [x] Run the crate tests to observe the missing behavior.
- [x] Implement mark/sweep from explicit roots. Keep object slot addresses stable; reclaim bytes and advance generation on reuse.
- [x] Run the crate tests and a deterministic randomized operation sequence that checks stats, handle validity, and root reachability.

### T3: Global `no_gc` guard and review receipt

- depends_on: [T2]
- owns: [`crates/cgx2-runtime/**`, `plans/memory/plan.md`]
- non_goals: [multi-threaded GC, final GC algorithm tuning]
- consumes: [region collector and allocation API]
- produces: [budgeted guard, counter-based conformance evidence]
- estimate: L
- coordination_risk: moderate
- status: complete

- [x] Add failing tests for zero collection across two auto regions in one guard, nested budgets, budget exhaustion before commit, explicit collect rejection, and guard drop after an error.
- [x] Run tests to see them fail, then implement budget charge/defer logic and RAII cleanup.
- [x] Run `cargo test --manifest-path crates/cgx2-runtime/Cargo.toml --offline`, `cargo fmt --manifest-path crates/cgx2-runtime/Cargo.toml --check`, and `cargo clippy --manifest-path crates/cgx2-runtime/Cargo.toml --offline -- -D warnings`.
- [x] Document exactly what this kernel proves and does not prove, including that byte buffers stand in for future generated trace metadata; report collection counters for every guard test.
- [x] Commit owned paths and stop. Report test results, commit ID, measured limits, and any API decisions needing integration review.

**Acceptance:** all listed tests pass; instrumentation shows zero CGx2 collections inside any guard; manual collection and limits behave as specified; no unsafe code or cross-track edit appears.

## Execution receipt

- T1/T2/T3 tests were observed red before implementation, then green. The final suite has 19 integration tests; the deterministic graph test runs 300 operations.
- The guard tests report `collection_count == 0` inside two-region, nested-budget, budget-failure, explicit-collect, and error-drop cases. Auto collection occurs at the next post-guard allocation safepoint; explicit collection after guard drop increments the manual region counter to 1.
- The installed `stable` alias lacks rustfmt and Clippy. All three specified checks pass with `RUSTUP_TOOLCHAIN=1.98.1`, the plan's pinned version.
- API decisions for integration review: `create_region` is fallible; `default_region()` exposes the built-in region; `link` wraps allocation failures in `MemoryError::Allocation`; fixed logical metadata charges are not an RSS measure. Unrooted handles can be invalidated by region close until the compiler's escape checker is integrated.
