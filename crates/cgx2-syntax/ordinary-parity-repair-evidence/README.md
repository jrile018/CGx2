# Implemented ordinary-memory evidence repair

This lane changes only harnesses, fixtures and reports. It starts from reviewed
repair base `7bb7a480119826693db67ed503caaf7f23f6abb2`. A red matrix is intentional
when the product violates a fixed expectation. This is not full ordinary-memory,
aggregate-ownership, promotion or release acceptance.

`common.py` uses the existing read-only process-group supervisor. Each ordinary
command has 180 seconds wall time, 120 seconds CPU, 4 GiB address space, 512 MiB
file size and 2 MiB retained output per pipe. Workspace/regression supervision
allows 3,600 seconds wall and 1,800 seconds CPU. These are per-process limits,
not aggregate descendant resource isolation. Signals, output overflow, timeouts
and spawn exceptions cannot satisfy an expected diagnostic. Command invocation
is persisted before spawning; results/checks are atomically persisted after
every operation. A disk failure may prevent the last write, but earlier durable
records survive. Unique attempt directories prevent overwriting prior evidence.

## Reproduce

From the attached worktree, using the existing Rust 1.98.1 installation:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1
export CARGO_TARGET_DIR="$PWD/target/release-ordinary-harness-hardening"
export TMPDIR="$PWD/target/tmp-ordinary-harness-hardening"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
python3 crates/cgx2-syntax/ordinary-parity-repair-evidence/test_harness.py
python3 crates/cgx2-syntax/ordinary-parity-repair-evidence/verify.py \
  --output-dir "$TMPDIR/new-verification"
python3 crates/cgx2-syntax/ordinary-parity-repair-evidence/run.py \
  --binary-dir "$TMPDIR/new-verification/frozen" \
  --output-dir "$TMPDIR/new-matrix" \
  --standalone-root /absolute/private/directory/outside-git
python3 crates/cgx2-syntax/ordinary-parity-repair-evidence/auto.py \
  --binary-dir "$TMPDIR/new-verification/frozen" \
  --output-dir "$TMPDIR/new-auto" \
  --standalone-root /absolute/private/directory/outside-git
python3 crates/cgx2-syntax/ordinary-parity-repair-evidence/replay.py \
  --binary-dir "$TMPDIR/new-verification/frozen" \
  --output-dir "$TMPDIR/new-replay"
python3 crates/cgx2-syntax/ordinary-parity-repair-evidence/package_parity.py \
  --binary-dir "$TMPDIR/new-verification/frozen" \
  --historical-product "$TMPDIR/new-replay/product" \
  --output-dir "$TMPDIR/new-package" \
  --standalone-root /absolute/private/directory/outside-git
python3 crates/cgx2-syntax/ordinary-parity-repair-evidence/review.py \
  --binary-dir "$TMPDIR/new-verification/frozen" \
  --matrix-report "$TMPDIR/new-matrix/report.json" \
  --output-dir "$TMPDIR/new-review"
```

Do not run Cargo after freezing. `verify.py` copies seven executables and five
canonical top-level probe libraries (including the runtime), records source
hashes, and uses the private dependency directory. It never chooses an arbitrary
hashed dependency as the probe library. `--prior-workspace-report` preserves an
already attempted workspace result while avoiding a retry of a blocked gate;
it does not turn a failure into a pass. `--skip-fault-native` is for exploration
only and explicitly fails the required fault-native row.

Standalone source and relocated executables are placed in the supplied private
outside-Git directory. The harness checks this with `git rev-parse`. Actual
rustc compilation and execution occur there, without project rlib search paths;
compiler temporary files still use assigned private TMPDIR. The fixed linker
thread wrapper is only supervision tooling. Source, toolchain/runtime hashes,
commands, errors, observations and binary hashes are retained. There is no
final-only in-memory standalone report. No host /tmp quota change is required.

## Accounting and coverage

`runtime_probe.rs` exercises existing public runtime controls. These results are
explicitly **direct-library evidence**, not source lowering evidence. On the
pinned 64-bit toolchain:

| Real limit | Exact successful budget | Zero / one-under expectation |
| --- | ---: | --- |
| Eight-byte object in Manual region | 8 + `SLOT_METADATA_BYTES` = 72 | `RegionLimit` |
| Allocation inside guard | 72 | `BudgetExceeded` |
| Region creation inside guard | `REGION_METADATA_BYTES` = 128 | `BudgetExceeded` |
| First scoped root | `size_of::<Handle>()` = 32 | `BudgetExceeded` |
| Nested fallible guard table | 2 entries × 2 `usize` = 32 | `BudgetExceeded` |
| One raw eight-byte graph copy | 439 | `BudgetExceeded`, no destination publication |

The copy budget is 6 handle slots (192), three hash buckets charged as
`size_of::<(Handle,u32)>() + 1` (123), the owned image header/payload/metadata
(12 + 8 + 32 = 52), and destination payload/slot (72). The four handle slots are
the temporary source root, traversal vector, preparation vector and staging
root. The initial guard-table allocation costs another 16 bytes **before** the
new guard is active. Hash-table capacity is toolchain-dependent; this formula
states the pinned Rust 1.98.1 behavior, not an allocator/RSS promise. Exact
successful total charge is checked, rather than calibrated from current output.

Nested guards cover two Manual regions, cumulative allocation charges, close
without refund, and zero collections after exit. Root cleanup is idempotent and
preserves primary result markers. The markers are library transport tests, not
synthetic demonstrations of real source contracts/arithmetic failures. Real
source fixtures independently execute normal return, early return, failed
argument evaluation, contract failure, arithmetic failure, and recoverable
allocation failure in interpreter, debug, release and standalone programs.
Cooperative cancellation drops an actual public root scope; Rust unwind uses
`catch_unwind` around a deliberate bounded panic. Aborts, external kill,
forgotten scopes and arbitrary host OOM are outside these guarantees.

`auto.py` additionally forces Auto trigger 1 under two nested guards in actual
source/interpreter/native/standalone execution. Its public runtime control checks
zero collection under guards and at exit, followed by exactly one collection at
the next Auto allocation while a Manual region remains untouched.

The direct reserve baseline reaches 16 reserves; the harness tests 1–17 and
records every site, source validity, destination bytes, root census and cleanup.
Session entry reserve 1 is separate. A straight-line source Session/guard/
two-region/copy fixture sweeps **all indices 1–64**. Its last affected reserve is
21; all higher indices must match the unaffected baseline. Indices 1–22 also
run native debug, native release and actual standalone. Error arms contain no
further managed allocations; this is a justified bound for this fixture, not
all possible programs. Synthetic reserve failures are separate from true byte
budgets. Actual identifier exhaustion and global OOM are **unrun**, never faked.

Every source run checks zero baseline/final census, balanced registered/released
roots and zero cleanup failures. OM07 cannot hide behind a zero final census.
Pause distributions are timing measurements and are excluded from deterministic
comparison. Only `region_id.session` is excluded as invocation identity; region
indices, charges, collection counters, failures, peaks, roots and every other
observation remain checked. No release performance measurements are claimed.

Static source/package negatives check exact codes and half-open UTF-8 byte
spans, including a multibyte prefix. Native build, Rust emission and Python
emission must preserve sentinel artifacts. Static dependency attribution must
name `lib.cg`; the current `main.cg` result stays failed. Actual package contract
errors and positive package values also execute through all four routes.

`review.py` replays OM01–OM08. It labels the OM06 result direct-library and
references the actual standalone OM07 result. Admission disposition checks in
that small review replay are distinct from the exact diagnostic tests in the
main matrix. `replay.py` executes the unchanged 117-check corpus, 18 regressions
and the separate historical shadow-builtin reconciliation. Historical expected
nonzero outcomes remain present in raw reports.

`export.py` writes compressed full command reports, inputs, emitted standalone
sources, all failed attempts and content hashes under `evidence/`. It omits large
rebuildable executables while preserving their hashes. See the adjacent
`S2B-ORDINARY-PARITY-REPAIR.md` for exact results and remaining gates.

## Hardened evidence acceptance

`validation.py` rejects missing, negative or non-integer root counters (including
booleans), unbalanced registrations, malformed/empty/duplicate direct rows,
and missing required commands or checks. `complete` describes required coverage;
a fully executed red product matrix remains non-accepting. The Session-entry
exception is exactly `session_entered: false, metrics: null` at reserve index 1.
All other routes require actual cleanup counters.

`freeze.py --verification-dir PATH --output-dir NEW_PATH` finalizes an already
built canonical candidate without Cargo and reloads the final harness snapshot.
`inputs.json` records complete dependency membership, hashes, inode/mode and
symlink identities, compiler/runtime/manifests, harness/fixture inputs and the
toolchain. Every product suite checks this inventory before and after execution.
Do not run Cargo or edit those inputs during product verification.

`replay.py` now also requires `--standalone-root` and runs the unchanged corpus
and all 18 regressions under that actual outside-Git directory. `quality.py`
requires `--historical-inventory` pointing to the preflight old-evidence hash
inventory. Export fresh runs only beneath `hardening-evidence/` with a distinct
output directory. The original `evidence/` directory remains immutable.

The copy expectation now includes the complete replacement cap-2 root table
needed while the source stays rooted through publication: `6*root +
3*(size_of::<(Handle,u32)>()+1) + 12+8+32 + alloc = 439`. Entering the guard
adds 16 bytes, for 455 total. Zero, exact 439, and one-under 438 still enforce
nonrefundable charging, source validity and failure atomicity. The original
375-byte failed observation remains retained.

The retained main selftest file has 48 tests (21 existing plus 27 added). Run
`python3 hardening-evidence/explicit_scenarios.py` from this directory for two
additional explicit prerequisite/sentinel cases. Together with the unchanged
8 historical harness tests, this is 58 passing Python selftests. The supplemental
tests mutate only disposable fixture copies and call production validators.
