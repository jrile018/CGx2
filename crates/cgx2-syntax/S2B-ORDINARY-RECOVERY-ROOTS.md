# Bounded ordinary-memory recovery root repair

Reviewed base: `89464a76c9bd84a3b090ec58a14ca6be0043c6ea`. Delivery branch: `riley/ordinary-recovery-roots`. The delivery commit is the Git commit containing this report and is reported in the chat. This is a bounded runtime repair for review. Full ordinary-memory and release acceptance remain open.

`graph.rs` now transfers an inline carrier's existing registration to its single bound payload when `Value.depth > 0` and both values have the same heap handle. Transfer completes after binding succeeds; `Env::pop` releases that obligation. Binding-free matches still release their scrutinee. Heap aggregate extraction keeps separate parent/child protection. Preparation failures release the scrutinee, and arm failures run binding cleanup.

CopyTo allocation recovery now belongs to `Machine::expr`, covering argument evaluation, graph copying and final result-root publication. `memory_expr` propagates allocation faults into that boundary. Recovery increments `Evidence.recovered` once and constructs the existing inline allocation Err. Primary diagnostics remain primary. Recovery ends before user arm execution.

The existing `contains_since(checkpoint, handle)` check, independent expression-result registrations, default allocation context restoration, public ScopedRoots semantics and safe copy source protection remain intact. No private scope helper, public interface, sema implementation, compiler interface, Cargo manifest/lock or other lane file changed. Returning a payload or an error carrier containing it retains a separate return obligation. No runtime charge was waived or budget increased.

Owned files are `crates/cgx2-runtime/src/graph.rs`, the new runtime/sema `tests/ordinary_recovery_roots.rs`, `ordinary-recovery-roots-evidence/**`, and this report. Serena-generated `.serena/` metadata is excluded from the commit.

## Verification

Rust 1.98.1, `CARGO_BUILD_JOBS=2`, offline/locked dependencies, private `target/release-ordinary-recovery-roots` and `target/tmp-ordinary-recovery-roots`, `LIBRARY_PATH=/tmp/cgx2-wave22-lib`, and bytecode-disabled Python were used. Commands have bounded wall/CPU/address-space/file/output supervision and persisted invocations/results. No Cargo command ran after the canonical freeze. Subsequent controls use rustc or clippy-driver against frozen libraries.

| Check | Actual result |
| --- | --- |
| Initial workspace all-feature tests | 1,337 passed, 34 permission failures, 6 ignored |
| Same built worker/HTTP executables with required local permissions | 11/0 and 24/0; replaces the affected suites, giving 1,371 unique covered workspace tests, 6 ignored |
| Workspace all-target/all-feature Clippy `-D warnings`, fmt, build | All exit0 |
| Final focused runtime/source tests | 4 runtime + 8 sema = 12 passed |
| Late binding-reassignment test | Included in the four frozen-library runtime tests; rustc `-D warnings` and clippy-driver pass |
| Fully hashed final runtime red/green control | Identical final tests: reviewed-base library 2 passed/2 failed; frozen repaired library 4 passed/0 failed |
| Original raw ordinary matrix | 957 passed / 2 failed; source recovery assertions pass |
| Independent recovery acceptance | 515 passed / 0 failed |
| Auto/no-GC | 29/0 |
| Original five-artifact API view | 84/0 |
| Original admission replay, actual external cwd | 196/0 |
| Package / review replays | 40/0 and 15/0 |
| Unchanged historical replay | 135 passed / 1 failed; all 18 regression commands executed |
| Separate legacy reserve reconciliation | 133/0, including all 120 recorded fault-route executions |
| Independent evidence-validator selftests | 3/0 |

The initial workspace failures were denied child lifecycle execution and loopback binding in unrelated worker/HTTP suites. Their raw failures remain. The same built executables passed with the needed local access; no replacement implementation or Cargo rebuild was used. The additional binding-reassignment test was added after the workspace run and verified directly against the frozen library; it is not silently included in the 1,371 workspace count.

The unchanged nested Session/two-region/two-guard source reaches **20** reserves on an unaffected execution. A diagnostic-only derived standalone trace records each site, and its value and deterministic memory evidence equal the canonical interpreter baseline. The original canonical matrix injects 1–64 and runs debug/release/relocated standalone at 1–21: **63 native fault executions**, covering every reached site and the first unaffected index. Independent checks also execute index64 through all three native routes, verify the full unaffected tail, and retain named10/11/21 regressions. Accepted outcomes remain exactly `-10, 0, -1, -2, -3, -4, -5, 42`; baseline42. No injected source reserve produces primary CGR090. The source IdentifierExhausted fallback `-11` is not accepted or claimed exercised.

Tests cover scalar and binding-free arms, heap aggregate extraction, repeated reads, multiple aliases, nested returns, returned heap/error payloads, replacing a transferred binding, argument evaluation once, arithmetic/argument failures, and an allocation failure in a separate user arm. That separate arm intentionally retains primary CGR090. Existing runtime root/lifetime/accounting, safe rooted/unrooted same/different-region copy, Auto trigger1, cycle/alias, rollback and cumulative no-GC budget suites remain covered by the workspace run. The new source copy-failure fixture checks source aliases remain readable after destination failure.

## Failures and reconciliation

The raw matrix's two failed rows remain **`direct:copy-budget-exact`** (historical375 expectation) and **`dependency-negative-exact-file`**. A separate expectation-only control uses the reviewed six-root sum: operation439, guard metadata16, observed total455. All50 direct controls pass, including zero/exact439/one-under438, source validity, rollback, balanced census and no refunds. The historical375 assertion remains failed; safe source retention and replacement-table charging were preserved.

The unchanged historical outer result is **135/1**, not136/0. Its managed-product subrunner retains **670/8**: graph-root index8 and guarded index9, through four routes each, expect an uncaught allocation at a binding registration that this repair removes. Independent diagnostic-only unaffected traces reach7 and8 respectively and match canonical baseline value/evidence. All120 original outcomes at indices1–15 through four routes match those reached-site bounds, including the remaining real uncaught temporary-read reserve. The separate133/0 reconciliation does not rewrite the original expectations, reports or nonzero exit. The older shadow-builtin historical negative/reconciled commands also remain in raw evidence.

Every exploratory/setup attempt is retained. These include initial test-fixture grammar mistakes, the demonstrated base reserve10/11 failures, a standalone copy failing on shared `/tmp` quota, the first legacy trace missing its managed-evidence flag, an API helper scratch-name collision, and a new explicit-return alias fixture rejected with existing CGT095 provenance checking. The rejected fixture's exact source is retained; the admitted tail-alias form passes all four routes without sema changes. An expanded12-artifact API view produced98 valid checks; the comparable original five-artifact view produces84. Its canonical API library bytes match a dependency hash recorded before the freeze.

Only four completed files owned by this lane were archived, hash-verified and removed from `/tmp` to relieve quota, including the exact partial failed binary copy. No other task scratch or host setting changed. The successful standalone/admission runs use the chat's private writable visualization directory outside every Git checkout. Reports retain actual resolved cwd, argv, exit/signal/reason, diagnostics, sources, artifact hashes and memory observations. No paid API, external contact, package installation, security-policy modification or release performance measurement was performed.

## Evidence and remaining limits

The committed `ordinary-recovery-roots-evidence/evidence/manifest.json` identifies compressed complete attempts, inputs, generated Rust, raw streams and byte/hash pairs. `attempts/audit.json.gz` contains the machine-readable final disposition. The audit verifies **12 canonical artifacts, 435 dependency/library metadata files and 205 compiler inputs unchanged** after frozen verification, with actual toolchain/runtime/source fingerprints. Large rebuildable canonical executables remain in ignored private scratch with recorded hashes; the quota archive preserves the failed partial-copy bytes.

`verify.py`, `products.py`, `run.py`, `legacy_audit.py`, `api_view.py`, `red_green.py`, `audit.py` and `export.py` retain runnable workflows using the existing read-only runners/supervisor. Use new output directories and unique API output basenames; exact successful retry argv are in `api-retry-03`, `recovery-02` and `legacy-reconciliation-02`. Expected original failures remain nonzero. Diagnostic-only trace controls are explicitly derived evidence, not substituted canonical binaries. Pause data is excluded from deterministic comparisons; process timing fields provide supervision only.

Static dependency filename attribution, nominal borrow constructor admission, existing harness completeness, general aggregate ownership/projections/loans, source copy_owned/promote same-owner recovery, full authenticated v3 summaries, identifier exhaustion, global allocator OOM, external leak-enabled assurance, independent audit and the full R01–R15 release requirements remain open. No broader source admission, general ownership-v3, release or downstream-consumer acceptance is claimed. Stop for review after this commit.
