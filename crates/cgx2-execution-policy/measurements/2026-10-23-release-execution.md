# S1-07 owned CPU execution delivery evidence

Date: 2026-10-23. Reviewed starting base:
`0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
Branch: `riley/release-execution`, attached worktree
`/home/john-riley/.codex/worktrees/217b/lang`.
The delivery commit is the commit containing this report; resolve its identifier
with `git log -1 --format=%H -- crates/cgx2-execution-policy/measurements/2026-10-23-release-execution.md`.

## Delivered scope

Only `crates/cgx2-execution-policy/**` changed. No dependencies, root manifests,
lockfiles, compiler paths or shared plans changed. The pre-existing untracked
`.serena/` is outside this delivery and is not staged.

| File | Result |
| --- | --- |
| `src/lib.rs` | Shared private engine for the existing scalar/zip checked i64 primitives; additive scalar cancellable APIs; fallible reservations/thread creation; explicit joining after spawn failure, panic, overflow or cancellation; legacy direct scalar clamping/fallback preserved |
| `src/owned_v1.rs` | Consumed Vec inputs, complete-result publication, error reports, exact operation/machine/size/worker/age-bound opaque profiles, structured measured costs and buffer/latency budgets |
| `src/profile.rs` | Existing evidence parser/builder/loader helpers exposed only within the crate for reuse; external borrowed APIs/schema unchanged |
| `src/bin/calibrate_owned.rs` | Operator-run quiet-host release calibration handoff, separate owned schemas, alternating paired samples, accepted-profile cost coverage, fresh evidence files |
| `examples/owned_pipeline.rs` | Executable library-only S2 adapter fixture for both operations with uncalibrated sequential Auto |
| `tests/owned_v1.rs` | Six integration tests spanning values, arithmetic/chunk boundaries, errors/cancellation, requested budgets, profile rejection, uncertainty and legacy evidence exclusion |
| `OWNED-V1.md`, `README.md` | Public contract, report schema, cost/ownership/cancellation boundaries and S2/benchmark handoffs |
| This report | Actual verification and outstanding acceptance |

No new arbitrary kernel, reduction, GPU backend, managed-session sharing or
source-language claim is introduced. Scalar multiply then add remains checked
in that order, including cases where a later mathematical addition could hide
an overflowing multiplication. Without cancellation, the lowest source error
index wins across chunks. Shared-engine deterministic fault tests cancel before,
inside and at the end of loops; they count completed workers after return and
inject allocation, panic and partial-spawn failures. These are explicitly
injected functional tests, not observed host OOM/thread-exhaustion evidence.

`owned_v1::execute` defaults only through existing `Policy::default()` = Auto;
`Budgets::default()` declares no limits. A missing/rejected profile cannot enable
parallel Auto. Exact-size owned map/zip schemas are 3/4 with workload IDs
`checked-i64-owned-map-v1` / `checked-i64-owned-zip-map-v1`; legacy borrowed schemas
1/2 are rejected. At least three paired calibration and holdout samples, bounded
noise, machine/clock/worker identity and full raw-data reconstruction are
required. The FNV checksum is local integrity checking, not authentication.

Reports carry machine, operation, elements, policy/profile status, decision,
validated empirical sequential/parallel costs, observed dispatch latency,
conservative requested i64 buffer accounting and budget assessments, including
on failure. Buffer limits reject preflight; elapsed limits report violations
without retroactive cancellation. Retained input capacity is charged; allocator
rounding, metadata, report storage and thread stacks are excluded. No exact RSS,
managed-region budget, deadline or GC guarantee is claimed by this buffer report.

## Verification actually run

Environment for Cargo compilation/execution:

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-execution"
export TMPDIR="$PWD/target/tmp-execution"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
```

Local machine fingerprint from the fixture: Linux x86_64, AMD Ryzen 7 9800X3D
8-Core Processor, 16 available threads,
`rustc 1.98.1 (48a229cea 2026-09-01)`. These diagnostics identify this functional
run; host quietness was not established.

| Exact command | Exit | Result |
| --- | --- | --- |
| `cargo test --offline -p cgx2-execution-policy --all-features` | 0 | **30 passed, 0 failed, 0 ignored**, including all existing policy/map/zip/profile tests, six new owned tests and shared-engine fault tests |
| `cargo clippy --offline -p cgx2-execution-policy --all-targets --all-features -- -D warnings` | 0 | Library, binaries, tests and example pass |
| `cargo fmt -p cgx2-execution-policy -- --check` | 0 | Owned-package formatting pass |
| `git diff --check` | 0 | Whitespace check pass |
| `cargo run --offline -p cgx2-execution-policy --example owned_pipeline` | 0 | Both 4,096-element operations return 13 at every index; Auto chooses sequential with MissingMeasurements; reports have expected 65,536/98,304 peak requested bytes; `source_pipeline_fixture=pass library_only=true` |

Logs are local ignored artifacts at `target/tmp-execution/tests.log`,
`clippy.log`, and `source-fixture.log`. No release timing dataset or measured
speedup is published here. Synthetic CSV strings in tests validate the evidence
protocol and must never be used as real performance evidence. The all-target
Clippy check compiles the quiet-host handoff; that timing runner was not run.

## Acceptance still open and executable handoffs

| Acceptance | Status | Required next action |
| --- | --- | --- |
| Rust library execution/cancellation/reporting | Passed bounded functional checks | Review this commit and contract |
| `.cg` source, typed admission, source spans, CLI performance JSON and notebook integration | Unrun / dependency S2 then S3 | Lower only the two proved-pure owned primitives through the documented API; translate errors/indices and include source input allocation/copy costs; run source/native semantic and cancellation fixtures |
| Real owned Auto selection and speedup/regret under calibrated conditions | Unrun / blocked on a quiet benchmark host | Run the commands in `OWNED-V1.md` below, retain raw/profile/commit/toolchain/host provenance, test loaded Auto against both explicit backends across sizes/worker counts/replicas |
| Integrated release R12/R13 performance/resource gates | Unrun / later bench-owner acceptance | Complete the preregistered source/reference study with uncertainty and full planner/copy/allocation/launch/join/output-lifetime costs; publish passed/failed results separately |
| GPU/general kernels/reductions/managed sessions | Unsupported in this contract | No implied fallback or extension; explicit GPU requests remain errors |

Quiet-host commands (not executed in this parallel completion chat):

```sh
cargo run --release --offline -p cgx2-execution-policy --bin calibrate_owned -- \
  --quiet-host-confirmed map 1048576 2 "$TMPDIR/owned-map.csv" "$TMPDIR/owned-map.txt"
cargo run --release --offline -p cgx2-execution-policy --bin calibrate_owned -- \
  --quiet-host-confirmed zip 1048576 2 "$TMPDIR/owned-zip.csv" "$TMPDIR/owned-zip.txt"
```

Use fresh paths and repeat independent runs rather than overwriting evidence.
The runner validates values, warms both paths, obtains bootstrap evidence, and
records seven calibration plus seven holdout samples per backend including
accepted-profile lookup/report costs. Invalid/noisy evidence fails closed. The
operator must record failures and contention, not relabel a rejected dataset as
a successful speedup. No other chat, dependent stage, full release study or
merge is started by this delivery.
