# Ordinary-memory evidence parity repair

Bounded evidence delivery for coordinator review. **Full ordinary memory and
release remain unaccepted.** No implementation or frozen interface was changed.
Base: `7bb7a480119826693db67ed503caaf7f23f6abb2`.
Branch: `riley/ordinary-parity-repair`.

Owned files are this report and `ordinary-parity-repair-evidence/**`: bounded
incremental Python harnesses, existing-public-API Rust probes, source/package
fixtures, selftests and compressed evidence. No coordinator ledger, historical
fixture/runner, root Cargo manifest/lock, or other lane was edited.

## Final verification

| Check | Actual result |
| --- | --- |
| Workspace all-feature offline/locked tests | Exit 101: 1,293 passed, 34 failed, 6 ignored |
| Workspace all-target/all-feature Clippy, warnings denied | Exit 0 |
| Workspace formatting | Exit 0 |
| Workspace all-feature offline/locked build | Exit 0; one explicit final build |
| New Python selftests | 21 passed |
| Unchanged historical harness selftests | 8 passed |
| Python syntax, diff check, compiler/runtime/manifests unchanged | Passed |
| New final matrix (`matrix-3`) | **902 passed, 75 failed**, 340 commands; no supervision exception |
| Actual package value/error parity (`package-1`) | **40 passed, 0 failed** |
| Forced Auto safepoints and deferred collection (`auto-1`) | **29 passed, 0 failed** |
| Unchanged ordinary corpus plus regressions | **136 passed, 0 failed**: 117 assertions + 18 commands + historical-negative reconciliation |
| OM01–OM08 review replay (`review-2`) | **1 passed, 14 failed**; all eight findings remain observable |

The workspace failures are in interop-worker lifecycle and notebook-UI runtime
contract targets under sandbox child lifecycle/exec denial (`Permission denied`,
including consequential assertions). Full logs remain in verification-1. They
were not retried or disguised as passes. Verification-2 references that same
failed test evidence, then runs Clippy, formatting and the final build. No host
security change or retry of a blocked external gate was performed.

All Cargo commands used Rust 1.98.1, offline/locked dependencies,
`CARGO_BUILD_JOBS=2`, private `target/release-ordinary-evidence-parity` and
`target/tmp-ordinary-evidence-parity`, and the existing
`LIBRARY_PATH=/tmp/cgx2-wave22-lib`. Seven executable and five canonical top-level
rlib hashes were frozen after Cargo. No Cargo command ran during product
verification. Final quality checks confirm compiler/runtime/manifests and all
frozen hashes unchanged. Toolchain binary/driver/LLVM/std hashes, runtime source
hashes, emitted source and executable hashes accompany actual commands.

The historical owned-Result runner still exits **1** for its single superseded
shadow-builtin negative (523 passed/1 failed). Its exact nonzero record is
retained. Reconciled checks are separate; no historical expectation was edited.

## What the matrix establishes and still fails

The 49 direct-library assertions pass 48 and fail OM06. Real zero/exact/one-under
limits cover allocation (72 bytes), Manual region limits (72), region metadata
(128), first root (32), nested guard-table growth (32), and raw graph copying
(375). These are byte-budget tests, not injected failures. Copy accounting is
specified in the harness README using runtime constants, `size_of` and the
pinned HashMap capacity; successful exact charge is independently asserted.
Nested guards, two regions, cumulative nonrefund after close, Manual exit,
idempotent cleanup, primary-result preservation, scope-drop cancellation and
bounded Rust unwind are covered through public APIs. Library primary-result
markers are labeled transport tests; real source failures run separately.

The direct fault baseline reaches 15 reserves; indices 1–16 cover every reached
site plus the first unaffected index. Session entry reserve 1 is a separate
pre-Session control. Records retain actual reserve site, error, destination
bytes, source readability, baseline/final root census and cleanup result.
Failed preparation publishes no partial destination in these checked cases.

The source sweep executes **all 64 indices**. Its straight-line program has
scalar-only failure continuations and a last affected index of 21; the remaining
tail must match the unaffected baseline. Indices **1–22** also execute native
debug, native release and standalone programs. Failed Session entry emits
`session_entered:false, metrics:null`, never invented zero counters. No actual
identifier-exhaustion or global-OOM evidence is claimed.

There are 33 actual standalone programs in the final main matrix, two package
programs and one forced-Auto guard program. Their emitted source, relocated binary and working directory
are outside Git repositories in this task's private visualization directory.
They are compiled with the installed toolchain without project rlib search
paths and run there. Rust compiler temporaries remain in assigned private
TMPDIR. Reports are written before dispatch and after each command/check,
including exceptions; the selftests verify partial evidence survives a simulated
quota/spawn failure. That simulation is not labeled an actual quota incident.
The earlier coordinator /tmp quota failure remains historical read-only evidence.

Actual source normal/early-return, failed-argument, contract, arithmetic and
allocation recovery cases run through interpreter/debug/release/standalone.
Package positive values and UTF-8 dependency contract errors also pass all four
routes. Exact source/package static negatives preserve native/Rust/Python
sentinels. Deterministic comparison excludes pause histograms and only the
session component of region identity. Region indices, charges, roots, peaks,
failures and collection counters remain checked. The forced-Auto fixture checks two matched guard entries/exits and zero protected
collections despite trigger 1. Its direct runtime counterpart checks zero at
guard exit and exactly one collection at the next Auto allocation, leaving the
Manual region untouched. No release timings are claimed.

The **75 failed assertions are not 75 distinct defects**:

| Category | Failed assertions | Actual evidence |
| --- | ---: | --- |
| OM06 | 1 | Safe same-region copy under Auto trigger 1 invalidates its source despite returning a destination |
| OM07 | 58 | Root registrations/releases differ despite zero final census; copy-cycle is 25/26, including actual standalone |
| Static dependency-file attribution | 1 | Correct `CGT095` UTF-8 span 223..225, but file is `main.cg`; required file remains `lib.cg` |
| Uncaught source reserve outcomes | 15 | Indices 10, 11, 21 return `CGR090 HostAllocation` rather than the expected scalar recovery; repeated semantic assertions across routes |

Those reserve failures retain exact spans respectively **0..1306**, **661..663**,
and **790..792**. All 66 native/standalone comparisons of actual primary error
identity/span versus interpreter agree, while their expected recovery result
checks stay failed. This distinguishes matching erroneous behavior from semantic
acceptance. Source metrics/terminal state are recorded; a source value cannot be
observed after a terminal error, so source readability in that situation is not
invented. Direct controls provide the independent source-validity/rollback test.

The separate eight-finding replay also retains OM01 missing authority rejection,
OM02 panic exit 101, OM03 accepted user-Err escape, OM04 rejected legitimate
borrow, OM05 NotRooted, OM06 source invalidation, OM07 counters and OM08 private
API leakage/missing public record. This lane does not repair their implementations.

## Evidence retention and remaining gates

`ordinary-parity-repair-evidence/evidence/manifest.json` hashes every compressed
report/input. `summary.json` lists every attempt. `matrix-1` preserves the initial
mixed-stderr/native diagnostic parsing mistakes and incomplete harness expectation
for outer guard fallback; `matrix-2` preserves the corrected parser but missing
`0` fallback expectation; final `matrix-3` uses the source-defined `0` fallback.
The original matrix-1 harness text was recovered and checked byte-for-byte
against its pre-run SHA-256, not approximated. Later attempts snapshot the
harness automatically. Earlier checks, failures and nonzero outcomes remain
present rather than rewritten. Reports include argv/cwd, exit/signal/reason,
stream byte counts/hashes and bounded stdout/stderr. Large rebuildable binaries
remain private with committed hashes; emitted standalone sources are compressed.

Required next sole-producer gates remain **unrun/incomplete**: general aggregate
ownership and projections/partial moves/reborrows/recursive/generic/package
ownership, source `copy_owned`/`promote` with readable/retryable same-owner failure,
complete authenticated v3 summaries, and precise dependency-file diagnostics.
This evidence does not convert library promotion into source support.

Actual identifier exhaustion, arbitrary allocator OOM, process abort/external
kill and forgotten scopes are not demonstrated cleanup guarantees. Leak-enabled
sanitizer evidence, independent memory audit, foreign/package/tensor and owned
map/zip integration, GPU/solver/isolation/registry availability, performance and
human/model studies, and all broader R01–R15/S3/S4 release gates remain required.
No blocked external gate was rerun. No packages, external contact/spending,
publication, merge, delegation or follow-on task was performed. Stop here for
coordinator review and combined repair rerun.
