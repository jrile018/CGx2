# Managed evidence parity repair

Repair base: `3c1cfe2aeba2f7135e4069709bf633c082d2aab0`.
This is a bounded harness/fixture delivery. The original managed delivery remains staged and unaccepted. It does not complete ordinary memory or a release gate.

## Result

The complete matrix is retained in `repair-attempts/final-verification-v3/managed-product/repair.json.gz`: **670 passed, 8 failed assertions**. The outer verifier correctly exits 1. The failures are the supplementary reserve-index-1 session-entry reproduction described below; none is counted as recovered success.

| Evidence | Targets | Result |
| --- | --- | --- |
| Real allocation budgets 0 / 225 / 226 | Interpreter, native debug/release, independent standalone | Passed |
| Real root bookkeeping budgets 0 / 63 / 64 | Same four paths | Passed |
| Nested outer 257 / 258, inner 226 | Same four paths | Passed |
| Cumulative nonrefund 451, hidden scalar/package allocation under budget 0 | Same four paths | Passed |
| Synthetic host reserve indices 2–15, graph/root expression and guarded expression fixtures | Same four paths, 112 observed outcomes | Passed |
| Supplementary synthetic index 1, both fixtures | Same four paths, 8 observed outcomes | Failed CLI source-span assertion and cross-target parity |
| Three historical static negatives plus UTF-8 reference-output case | Check, native debug/release refusal, Rust/Python emission refusal | Passed exact code/message/byte-span checks and all 16 artifact sentinels |
| Arithmetic, negative argument index, requires contract, failed arguments | Same four execution paths | Passed exact primary diagnostic/span and deterministic memory/cleanup parity |

All **44 real-budget executions** returned their expected value with exact recovered counts, zero collection counts under the actual guards, balanced roots, zero cleanup failures, and baseline/final root census zero. The 64-byte root fixture deliberately uses the existing interpreter test's discarded recovery carrier: matching its successful reference inside the guard would require additional root bookkeeping and test a different budget. The other budget fixtures distinguish BudgetExceeded (42), HostAllocation (43), other allocation error variants, and success (7) through source matching. Full deterministic observations agree across the four paths; only session identity and pause timing are normalized.

The required synthetic 2–15 outcomes are **40 recovered inside-expression successes**, **4 uncaught guard-entry failures**, **16 uncaught runtime failures outside recovery**, and **52 indices not reached**, across all four paths. These are synthetic reservation faults, not real budget evidence or proof that an unreached index was exercised. Error exits retain CGR090 and exact primary spans; signals, output limits, unexpected exits, wrong results, and wrong classification fail. No error exit is credited as a recovered allocation.

## Remaining implementation defect (core-auth owner)

The existing feature-gated call
`cgx2_cli::run_checked_source_with_failure_at_reserve(source, &[], 1)`
with `repair-fixtures/fault-graph-root.cg` returns:

```text
CGR090: managed session entry failed: HostAllocation { site: SessionRegions }
span: None
```

Native debug/release and independently compiled emitted Rust return the same code/message with span **0..592**. The guarded fixture similarly has missing CLI span versus **0..687**. Each interpreter span assertion fails, and each of the three native/standalone comparisons fails: **8 failed assertions total**. This reproduces the existing review item; recovery of session/guard entry remains deferred. No implementation or public hook was changed in this lane.

Commands, stream bytes/hashes, diagnostics, exit/signal/reason, observations, source snapshots and binary hashes for the minimal reproduction are in the final matrix rows labeled `fault-graph-root-1:*` and `fault-guard-1:*`. The adapter is `fault_driver.rs`; it calls only the frozen CLI test API. Native builds use `--test-fail-reserve`; standalone compilation uses the frozen `feature="fault-injection"` and compile-time `CGX2_TEST_FAIL_RESERVE` hook.

## Runner and checks

`run.py --repair-only` runs the bounded repair; its default full run also runs the repair and strengthens the historical rejection loop. `repair_runner.py --suite budgets,faults,rejections,runtime-errors` permits isolated reruns. Attempt directories must be new; private temporary directories are unique and retained. Failed rows do not stop independent matrix cases.

`verify.py` defaults to the offline/locked CLI/compiler build with the CLI `failure-injection-test` feature, 14 adversarial runner selftests, the repair matrix, Python syntax, Rust adapter formatting, and staged/unstaged whitespace checks. Historical regression commands remain explicit opt-ins. Leak/ASan runners are excluded from this repair entry point. The original superseded `shadow-builtin:reject` negative remains separate in unchanged `summary.json` and historical evidence; its optional verifier status remains `known-superseded-negative`, never a new managed pass.

Build/temp paths are private `target/release-managed-evidence-repair` and `target/tmp-managed-evidence-repair`. Environment: Rust 1.98.1, `CARGO_BUILD_JOBS=2`, `LIBRARY_PATH=/tmp/cgx2-wave22-lib`. Matrix subprocess limits: wall 180 s, CPU 120 s, address space 4 GiB, file size 512 MiB, live nonblocking stdout/stderr cap 2 MiB each. The outer verifier uses wall/CPU 900 s, 4 GiB, 512 MiB files and 8 MiB merged output. Limits apply per process, not aggregate cgroup isolation. No performance measurement was run.

Executed commands:

```sh
python3 crates/cgx2-syntax/default-managed-evidence/verify.py --output-dir crates/cgx2-syntax/default-managed-evidence/repair-attempts/final-verification-v3
python3 crates/cgx2-syntax/default-managed-evidence/verify.py --output-dir crates/cgx2-syntax/default-managed-evidence/repair-attempts/final-quality-v2 --only build,repair-selftests,python-syntax,rustfmt-driver,diff
```

The first command exited 1 for the known session-entry defect. The second exited 0: offline/locked build, **14/14 selftests**, four Python syntax checks, Rust adapter formatting and diff checks passed. The final adapter formatting change is whitespace only; the matrix retains its compiled pre-format source snapshot and hash. `final-formatted-driver` separately compiles the final adapter and samples indices 1/2/8/15 on both fixtures (17 passing checks, the same two missing-span failures). `final-budget-format` reruns all real-budget cases after removing a trailing blank line from the new package fixture (167 passing checks, zero failures). Final verifier edits restore optional historical commands and add formatting/staged-diff checks; matrix behavior is unchanged. No workspace-wide tests, leak-host controls, external audit, or performance gates were rerun.

## Retained attempts

- `initial-budgets-rejections`: 238 passed / 17 failed. Initial harness assumptions incorrectly expected a second matched reference to fit the 64-byte root budget, used a function-start offset for a whole-module UTF-8 diagnostic, and expected package compiler fail-fast output to contain the check command's full diagnostic list. The original attempted source bytes are retained under this attempt's `sources/`; corrected expectations preserve the specified real budgets and exact published primary diagnostics.
- `initial-faults`: 397 passed / 0 failed for required indices 2–15, before supplementary index 1 was added.
- `final-verification`: retained infrastructure failure. The outer 64 MiB file hard limit prevented children from applying the documented 512 MiB limit, yielding explicit spawn errors. The outer limit now agrees with the documented child limit; it was never removed.
- `final-verification-v2`: retained private temporary directory collision from the preceding attempt. New attempts now use unique private temporary directories.
- `final-verification-v3`: complete final matrix, with the eight honest session-entry failures.
- `final-quality`: initial code quality checks passed. A later staged whitespace check caught the new package fixture's trailing blank line; its failure is retained in `staged-whitespace-first.log` and the blank line was removed.
- `final-formatted-driver` and `final-budget-format`: final formatted source checks described above.
- `final-quality-v2`: final staged code quality checks passed.

Large new reports are stored as lossless `.json.gz` files. `repair-artifacts.json` records stored and decompressed hashes; source snapshots and bounded raw command output remain retained.

Root manifests, implementation files, shared plans, original fixtures, historical attempts and `summary.json` are unchanged. The existing leak negative-control host block remains open. Coordinator rerun on combined repaired code and acceptance review are still required; no dependent consumer work, main merge, publication, or follow-on task is authorized here.
