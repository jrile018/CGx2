# S1-02 fallible runtime and lifetime completion

Delivery on `riley/release-memory` from reviewed base
`0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`, 2026-10-23.
The enclosing Git commit is the delivery identity (`git log -1 --format=%H --
crates/cgx2-memory-conformance/release-memory.md`). No dependencies or shared
manifests/lockfiles changed. Only the two assigned crates are part of this delivery.

## Implemented scope and invariants

The [runtime README](../cgx2-runtime/README.md#fallible-entry-and-lexical-roots-s1-02)
specifies all added signatures, reserve sites, defaults and accounting. This is
**Rust library support for S2 lowering**, not ordinary `.cg` function lifetime
support. Existing source execution and compiler-owned APIs are unchanged.

- Fallible `Session::try_new` and `try_no_gc` expose constructor/guard bookkeeping
  failures. `SessionEntryError` distinguishes reserve failure from exhausted
  IDs. The new `ReserveSiteV2` leaves existing exhaustive enums frozen. Constructor
  injection is available before a session exists; later new and old reserves
  share the existing one-shot fault counter with a versioned failure-site query.
- `ScopedRoots` tracks each successful root, protects its registrations from
  manual `unroot`, and cleans up on nested scope exit, return/error, cooperative
  cancellation/drop and panic unwind. Cleanup drains once, never reserves or
  collects, and continues after diagnostic failure. `ScopeOutcome` retains the
  exact caller result separately from cleanup diagnostics. Manual registrations,
  stable handles, region limits and existing ownership conversions keep their
  behavior. Unrooted managed objects remain until explicit/manual or permitted
  automatic collection; root cleanup is not a region-close operation.
- New budget/root tracking tables check requested and actual-capacity byte
  charges before commitment. Full replacement allocations charge every active
  budget cumulatively; cleanup does not refund allocation budgets. Guard-table
  growth charges outer guards before the new guard becomes active. All region
  collection stays forbidden inside guards, including after failed entry,
  failed root reserve or failed object allocation in nested guards.
- Legacy `new`/`Default`/`no_gc` intentionally retain their infallible entry and
  historical bookkeeping policy. An initial compatibility regression exposed by
  the old state corpus was corrected by keeping the new policy opt-in through
  `try_new` and `try_no_gc`. Their old corpus now passes unchanged.

These guarantees do not cover allocator aborts, process-wide OOM, Rust/std
diagnostic/report allocations, foreign/Python/PyTorch/device storage, forgotten
scopes, replacing the borrowed session, panic-abort or killed processes. Fixed
logical byte charges are not physical RSS. Compiler binding escape checks and
foreign retention remain the responsibility of their assigned integration stages.

## Files and corpus

Runtime changes: `src/lib.rs`, `budget.rs`, `fault.rs`, `heap.rs`, new `scope.rs`,
`tests/fallible_lifetimes.rs`, and `README.md`. The new test cases cover first and
nested guard reserve faults, first and growing root tables, invalid/stale handles,
root-count and ID overflow (internal controlled fixtures), preserved original
errors when cleanup reports failure, duplicate/manual/protected roots,
cross-region isolation, repeated cleanup, nested exits and both inner/outer
budget exhaustion without refund.

Conformance changes: `tests/fallible_lifetime_model.rs`,
`corpus/release-lifetimes.csv`, `scripts/accept_lifetimes_asan.py`, `README.md`, this
report and `evidence/release-memory/**`. Six fixed traces cover manual/auto modes
and return/cancel/unwind. An independent count oracle checks per-region and total
root census after failure/success/cleanup transitions, zero collection counts,
manual-region liveness until explicit collection, and stale handles afterwards.
Entry/reserve, invalid-handle and bookkeeping-budget failure transitions are
replayed; inaccessible counter-overflow/corruption branches have internal tests.

## Verification

All commands run from the attached worktree. Environment:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-memory"
export TMPDIR="$PWD/target/tmp-memory" LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test -p cgx2-runtime -p cgx2-memory-conformance --all-features --offline
cargo test -p cgx2-runtime --no-default-features --offline
cargo clippy -p cgx2-runtime -p cgx2-memory-conformance --all-targets --all-features --offline -- -D warnings
cargo fmt -p cgx2-runtime -p cgx2-memory-conformance --check
git diff --check
```

Results: **145 passed, 0 failed, 0 ignored** in the combined all-feature package
tests, including doc tests; **77 passed** in the runtime no-feature tests.
Clippy, package formatting and whitespace checks exit **0**. These test counts
are tests, not claims about the number of source programs or external trials.
See [machine-readable verification](evidence/release-memory/verification.json)
and the adjacent retained logs (terminal blank lines omitted in saved text).

## Sanitizer results and external handoff

```sh
python3 crates/cgx2-memory-conformance/scripts/accept_lifetimes_asan.py \
  --output "$PWD/target/release-memory/asan-leaks"
python3 crates/cgx2-memory-conformance/scripts/accept_lifetimes_asan.py \
  --output "$PWD/target/release-memory/asan-no-leaks" --detect-leaks 0
```

Leak-enabled attempt: **blocked, exit 2**. Runtime/probe builds succeeded with
Rust 1.98.1, `RUSTC_BOOTSTRAP=1`, `-Zsanitizer=address`, `-Copt-level=1`,
`-Cdebuginfo=1`, target `x86_64-unknown-linux-gnu`, and effective
`ASAN_OPTIONS=detect_leaks=1`. The deliberately leaking control exited 1 with
`LeakSanitizer has encountered a fatal error` and its ptrace restriction hint.
The leak-enabled corpus therefore remains unrun, not passed. Its
[result](evidence/release-memory/asan-leaks/result.json) and
[diagnostic](evidence/release-memory/asan-leaks/leak-probe-run.stderr) are retained.

Separate non-leak ASan: **passed, exit 0**, effective `detect_leaks=0`; **12 tests**
across runtime unit tests and both new lifetime suites passed. See
[result](evidence/release-memory/asan-no-leaks/result.json). This is address
instrumentation of the runtime/test crates using the installed sanitizer runtime,
not an instrumented standard library or leak acceptance.

On an external capable host, check out this delivery and run the first command
with an installed Rust toolchain and a private writable output directory. The
runner installs nothing, changes no host policy, and first requires the known
leak to be detected. It records effective ASan/LSan configuration, source SHA-256,
toolchain identity, exact build/run commands, exit codes and stdout/stderr.
Missing tooling, timeouts or host sanitizer restrictions return blocked (2),
test/compiler failures return failed (1), and only successful controls and corpus
return passed (0). There is no automatic fallback to non-leak mode.

## Remaining acceptance

R03/R13 release acceptance stays open: external leak-enabled execution and
independent safety audit are still required. S2 must integrate the new fallible
entry and lexical roots into ordinary managed/owned function lowering, keeping
bindings rooted across allocating safepoints. S3 must adopt cleanup for notebook
state, cancellation and foreign retention. Full supported source/foreign corpus,
release performance studies and external audits are unrun here. No benchmark,
host-policy change, package installation, compiler edit or dependent-stage work
is part of this delivery.
