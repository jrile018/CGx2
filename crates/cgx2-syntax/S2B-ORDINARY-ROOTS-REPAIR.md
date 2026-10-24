# Bounded ordinary-memory runtime roots repair

Repair base: `7bb7a480119826693db67ed503caaf7f23f6abb2`.
Branch: `riley/ordinary-roots-repair`, attached private worktree.
This delivery repairs OM05, OM06 and OM07 only. It does not accept the original
ordinary-memory delivery, complete R03/R04, or accept a release.

## Behavior and ownership

`crates/cgx2-runtime/src/graph.rs` now executes a named region arm against the
same `ScopedRoots` registration table as its function environment. An outer
binding's assignment releases the old registration from its actual owner and
retains the replacement for the outer binding's lifetime. The existing lexical
environment still releases arm-local bindings immediately. The new crate-private
`ScopedRoots::cleanup_region_v1(region)` in `scope.rs` removes only registrations
dependent on the closing dynamic region, without allocating or collecting. It
continues cleanup and reports errors; it does not swallow `NotRooted`. Result
escape validation still runs before storage closes. Existing source provenance
checking still rejects storing a region-local graph in an outer default cell.
Overwritten outer graphs are collectible before the region arm exits.

`ScopedRoots::copy_graph_to_v1(destination, source)` now keeps a temporary source
registration across graph snapshot, every destination allocation, and publication.
It removes that temporary on success or error and retains exactly one successful
copy publication registration in the caller's scope. A valid unrooted input is
supported by the existing safe API, including same-region Auto `trigger_bytes: 1`.
Source bytes, aliases and cycles survive the call and all tested failures. The
source's original rooting status is restored; a caller needing survival across
later independent collections must still own an appropriate root. Destination
preparation/publication remains transactional and failed reservations receive no
budget refund. The existing promotion/failed-owner interfaces are unchanged.
OM06 was reproduced through the direct safe runtime API; the source evaluator
already roots expression arguments, so this is not claimed as a source-level
OM06 reproduction.

The source evaluator now counts the root actually published by a successful
copy. No count is normalized and no release is omitted. Region results already
belong to the shared table and no longer need a redundant cross-table root.
All public enums, model layouts, signatures, region brands, API defaults and
root obligations remain unchanged. Default region policy remains Auto, 1 MiB
trigger and 64 MiB limit. No Cargo manifest/lock reconciliation is required.

## Verification

Rust 1.98.1, offline/locked dependencies, `CARGO_BUILD_JOBS=2`, private
`target/release-ordinary-runtime-roots` and `target/tmp-ordinary-runtime-roots`.
The immutable command attempts and exact limits/outputs are under
`ordinary-roots-repair-evidence/checks/`; final raw records and their hashes are
under `ordinary-roots-repair-evidence/evidence/`.

| Check | Result |
| --- | --- |
| Runtime, sema and memory-conformance all-feature suites | 357 passed, 0 failed, 0 ignored |
| New runtime/source tests, included above | 4 runtime + 8 sema tests passed |
| All-target/all-feature Clippy for those three packages, `-D warnings` | Passed |
| Workspace format check and owned diff check | Passed |
| Final offline/locked all-feature workspace build | Passed |
| Existing ordinary-memory evidence harness selftests | 8 passed |
| Interpreter/native debug/native release/standalone focused parity | 80 passed, 0 failed |
| Unchanged outside-repository standalone assertions | 22 passed, 0 failed |
| Unchanged ordinary-memory product runner and regression parents | 136 passed, 0 failed (117 direct + 19 regression assertions) |

The new direct-runtime corpus covers Manual/Auto, same/different regions,
unrooted cyclic/aliased inputs, zero/exact/one-under guard budgets, empty
destination limits, and all 54 reachable copy reserve failures across the four
mode/destination combinations. Every injected failure preserves source bytes and
topology, restores the root census, rolls destination committed storage back to
baseline, and permits retry. A fresh root table separately exercises source
protection and publication-capacity reserves. Source tests also run 128 reserve
index executions across nested assignment and cycle-copy programs; these include
indices beyond their last actual reserve and are not claimed as 128 distinct
fault sites. Normal, early-return and arithmetic-error paths balance roots and
guards, close named regions, and preserve the original `CGR006` error.

The shipped assignment now reports 3 registered / 3 released; copy-cycle reports
26 / 26 (formerly 25 / 26); failed copy reports 11 / 11. All have baseline/final
root census zero and cleanup failures zero. Six fixtures agree across all four
execution paths, including exact arithmetic error byte spans. The direct reviewed
`copy-root.rs` probe links the canonical frozen `libcgx2_runtime.rlib` and reports
both source and copy bytes `[42, 0, 0, 0, 0, 0, 0, 0]`, with one collection.

All Cargo writes finished before freezing seven compiler/consumer binaries and
five canonical top-level probe libraries. Runtime/compiler input hashes,
dependency hashes, rustc/Cargo identities and standalone sources are retained.
Final verification checks these hashes again; no Cargo build runs during parity.

## Retained failures and remaining limits

Baseline tests reproduce `NotRooted`, same-region source invalidation, and the
25/26 root evidence mismatch. A first test compile used a private module path;
its failed output is retained before the corrected test reproduced the defect.
The initial format failure is also retained. The first product attempt passed
117 direct assertions but failed 19 regression-related assertions before child
launch: the outer supervisor's 900-second hard CPU limit was below the unchanged
regression parent's 1,800-second limit. The retry changes only the outer limit to
2,400 seconds; all child assertions/limits and frozen artifacts are unchanged.
These failed attempts remain failures in the evidence.
The completed retry passes all 136 assertions. The historical owned-Result suite's
single obsolete shadow-builtin negative remains separately recorded as 523 passed /
1 failed, with the required exact reconciliation (593 + 17 passed); it was not
deleted or relabeled as a passing negative.

The standalone helper's literal `/tmp` scratch placement is redirected to a
private writable directory outside the repository. Its code and assertions are
unchanged; an independent journal persists every attempted command. The previous
coordinator's quota traceback and scratch audit are retained as historical
evidence, rather than discarded after the successful private-directory run.

Independent source/API probes retain five unresolved failures: OM01 accepts the
shadowed collection fixture; OM02 panics with exit 101; OM03 admits the user-Err
escape; OM04 rejects the legitimate borrow function with CGT095; OM08 exposes the
private function through ordinary API output. Those lanes and all coordinator
ledgers remain unedited. General aggregate ownership, source copy_owned/promote
with failed-owner continuation, complete authenticated ownership-v3 summaries,
and precise static dependency-file attribution remain incomplete.

The full workspace **test** suite, external leak-enabled assurance, independent
safety audit, and release performance measurements were not run for this bounded
repair. Package tests cover existing runtime region/graph/ordinary/conformance
suites; the final build covers workspace consumers. No package installation,
delegation, other chat, merge, publication, external contact/spend, host-security
change or follow-on task was performed. Coordinator review is still required.
