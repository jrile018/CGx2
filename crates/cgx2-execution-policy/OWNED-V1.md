# Owned CPU execution contract v1

`owned_v1::execute(input, policy, context, cancellation, budgets) -> Outcome`
is the additive S2 host-adapter seam for the two existing checked primitives.
[The executable fixture](examples/owned_pipeline.rs) uses both operations. This
is a Rust library contract; `.cg` syntax, typed IR admission, lowering, CLI JSON
and notebook adaptation belong to the later compiler/consumer stages.

`Input::Map { values: Vec<i64>, multiplier: i64, addend: i64 }` computes
`values[i].checked_mul(multiplier).checked_add(addend)`.
`Input::ZipMap { left: Vec<i64>, right: Vec<i64>, addend: i64 }` computes
`left[i].checked_mul(right[i]).checked_add(addend)`. Multiplication overflow
remains an error even when the subsequent addition could bring a mathematical
result into range. No reassociation, reduction, floating-point kernel, callback,
GPU, managed session or foreign handle is admitted.

Move the vectors into `Input`. No input clone is made by execution. The returned
`Outcome.result` is either a complete ordered `Vec<i64>` or `Error`; the separate
`Outcome.report` also exists on failure. No partial vector is published. All
input vectors are dropped and all launched workers joined before return.
Copying a borrowed source array into these vectors is the source adapter's
explicit allocation/copy cost and must be measured in source end-to-end studies.
The library cannot prove source purity, effects, aliasing or region lifetimes.
S2 must establish those properties, exclude shared managed sessions and `no_gc`
workloads, enforce source bounds, and translate errors/indices to source spans.

## Planning and cancellation

`policy` uses the existing `Policy` (`Default` is `Auto`). Explicit `Gpu` returns
an unsupported plan error. `Context.parallel_workers` is the host's requested
count. Explicit parallel requires 2..=32 workers, at least 2,048 elements and at
least 1,024 elements per worker. Auto uses the existing bounded worker planner
and stays sequential without accepted, beneficial evidence.

Validation precedence is unequal zip lengths, infeasible explicit workers,
backend planning, cancellation, then requested buffer budget. Once running,
cancellation observed at a poll/final publication check wins over arithmetic or
worker errors. The host sets the shared `&AtomicBool` to `true` and leaves it set
until return. Polls occur before allocation, every 256 elements per worker, and
at completion; this is cooperative cancellation, not an OS deadline. Thread
creation and joins can take unbounded host scheduling time. Completion races a
late request at the final check; a request after it cannot revoke publication.
Without cancellation, errors from the first failing input chunk win, and checked
arithmetic returns the lowest overflow index. Each chunk computes in source
order. Worker panic maps to `MapError::WorkerPanicked { chunk }`; allocator or
thread creation resource refusal maps to `MapError::AllocationFailed`. Every
already launched worker is explicitly joined on spawn refusal as well.

The legacy `execute_map`, `execute_zip_map`, `Execution`, errors and planner
schemas remain available. Legacy direct `map_i64_checked_parallel` keeps its
worker clamping and small-input sequential fallback. New
`execute_map_cancellable`, `map_i64_checked_sequential_cancellable` and
`map_i64_checked_parallel_cancellable` add the scalar counterpart of the existing
zip cancellation APIs; the new direct cancellable parallel map is strict.

The existing `execute_zip_map_cancellable(left, right, addend, policy, caps,
cancellation)` remains the borrowed S2 consumer contract: slices must outlive the
call, be equal-length, and contain checked independent i64 values. It allocates
an output vector and, in parallel, private chunks and a handle vector, launches
scoped CPU threads, copies each successful private chunk into the output once,
and joins every thread before returning. Inputs are borrowed without copying;
the caller owns any source-to-slice construction/copy and its cost. The owned
facade uses the same private two-primitive engine and additionally owns input
cleanup/reporting. No managed runtime sessions are shared across threads.

## Evidence and selection

Capture `profile::current_machine()` locally once for `Context.machine`.
The fingerprint includes OS, architecture, CPU, available threads and rustc.
`Context.profile` is `None` unless `ValidatedProfile::load(operation, text, csv,
machine, workers, now)` succeeded. Fields of `ValidatedProfile` are private;
legacy caller-constructed `Measurements` cannot enter this API. The raw CSV and
profile text must be retained together and trusted locally: the FNV checksum
checks accidental changes, not authenticity or independent review.

`owned_v1::build_profile(operation, csv, machine, workers, elements, now, ttl)`
produces an exact-size profile. Owned map uses CSV schema **3** and workload
`checked-i64-owned-map-v1`; owned zip uses **4** and
`checked-i64-owned-zip-map-v1`. Borrowed schemas 1/2 cannot enable owned Auto.
The builder/loader reuse the existing paired-evidence validator: at least three
matching repetitions per backend in calibration and holdout, no duplicates,
positive timings, at most 2x spread per backend/phase, exact raw checksum and
reconstructed costs/metadata, TTL between one second and seven days. The
calibration command uses 24 hours. Intervals include every sample with a 10%
high-side margin. Parallel wins only when its upper bound is strictly below the
sequential lower bound. Overlapping intervals remain sequential. The margin is
an empirical precaution, not a statistical confidence bound or future guarantee.

Every dispatch rechecks operation, **exact** element count, requested workers,
machine and creation/expiry against the clock. A rejected handle is reported as
`ProfileStatus::Rejected(Operation|Machine|Size|Workers|Age)` and supplies no
measurements to planning. Missing evidence is `Missing`; accepted evidence
records its checksum and calibration/holdout counts. Loading malformed, noisy,
insufficient or tampered evidence fails with `ProfileError`; the host records
that error and uses `profile: None`. `execute_at(..., now)` is a deterministic
trusted-host clock seam; ordinary callers use `execute`. A forged machine or
clock supplied by a caller is not host attestation. Changing contention, thermal
state or the execution implementation requires new calibration, even if a
fingerprint still matches. No local functional fixture establishes a speedup.

## Report and budgets

`Report.schema_version = 1` is a typed Rust schema. `operation`, `elements`,
`machine`, `policy`, `profile`, and optional `decision` identify the invocation.
The existing decision includes selected backend, reason, worker count and cost
status. `measured_costs` separately carries validated sequential/parallel
end-to-end empirical intervals, including when policy is explicit sequential.
`elapsed_ns` is an `Instant` observation from dispatch entry through planning,
worker launch/computation/join and input cleanup. Final fixed report assembly is
outside that observation. Calibration measures around the entire `execute`
call, including report assembly; profile loading, machine probing, input
preparation and output consumption/release are outside that timed call.

`Budgets::default()` sets `elapsed_ns: None` and
`requested_buffer_bytes: None`. Declared latency is observational: the result is
retained and `elapsed_budget` is `Within` or `Exceeded`; exceeding it never
pretends to cancel or roll back a successful call. The optional requested buffer
limit rejects before dispatch allocations with
`Error::BufferBudgetExceeded { required, limit }`. Equality is admitted. Both
limits are `u128`; byte accounting widens lengths/capacities before arithmetic.

`Buffers` records input **capacities** in bytes, requested final output bytes,
requested worker chunk bytes (zero sequential, one full output parallel), and
their conservative peak sum. This describes requested i64 storage, not allocator
rounding, allocator metadata, vector/handle/report storage, thread stacks, RSS or
managed-region/no_gc accounting. Source construction, machine probe/profile
parsing and OS/runtime allocations are additional costs. On early failure it is
a planned upper request bound, not evidence the buffers were allocated.
`buffer_budget` is `NotEvaluated` for a declared limit when validation/planning
fails, otherwise `NotDeclared`, `Within` or `Exceeded` as applicable. Reports
make no GC pause or exact RSS claim.

## Executable handoffs

From the attached worktree, with Rust 1.98.1 available:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-execution"
export TMPDIR="$PWD/target/tmp-execution"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo run --offline -p cgx2-execution-policy --example owned_pipeline
```

On a **quiet calibrated host only**, with fresh output paths, collect real
release-build evidence for each operation and supported size/worker count:

```sh
cargo run --release --offline -p cgx2-execution-policy --bin calibrate_owned -- \
  --quiet-host-confirmed map 1048576 2 "$TMPDIR/owned-map.csv" "$TMPDIR/owned-map.txt"
cargo run --release --offline -p cgx2-execution-policy --bin calibrate_owned -- \
  --quiet-host-confirmed zip 1048576 2 "$TMPDIR/owned-zip.csv" "$TMPDIR/owned-zip.txt"
```

The runner verifies both values and warms both backends, builds bootstrap
evidence, then times seven calibration and seven held-out repetitions per
backend, alternating backend order. Recorded runs include accepted-profile
validation/report costs; bootstrap samples are not mixed into the saved CSV.
A noisy bootstrap or final dataset fails closed. No timer runs during ordinary
execution to create a profile automatically. Keep the raw CSV, profile, exact
commit/toolchain and host provenance for independent review. Repeat across
small/large inputs, host contention conditions and independent replicas; test
loaded Auto decisions against both explicit backends. The bench owner must
include source construction/copies, full output lifetimes and source planner
costs in the later integrated runtime gate. This harness does not close R12/R13
or replace the preregistered quiet-host release study.
