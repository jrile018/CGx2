# S2b ordinary-memory core-admission repair

Repair base: `7bb7a480119826693db67ed503caaf7f23f6abb2`.
Branch: `riley/ordinary-core-admission-repair` in the attached private worktree.
This is a bounded OM01–OM04 repair for coordinator review. It does not accept
ordinary memory or the release. No shared interface, coordinator ledger, root
Cargo manifest/lock, runtime, API, or other lane was changed.

## Implementation

- **OM01:** `cgx2-sema/src/ordinary_memory/mod.rs` assigns compiler-only
  `$cgx2$ordinary_memory$alloc$N` and `$cgx2$ordinary_memory$collect$N`
  identities to real operations. `cgx2-sema/src/effects.rs` seeds their resolved
  summaries with `ManagedAlloc`, or `Collect` plus `MemoryCollect`, respectively.
  The existing call/SCC closure propagates both dimensions, including concrete
  generic instances and imported/transitive callers. Original source/AST inputs
  cannot declare or address these identities. `default_region` remains resource
  access without a new allocation requirement. No requirement derives from
  `managed.ref` or `managed.collect` template calls; those calls were removed.
  User `managed.*` declarations keep their real signatures/effects/capabilities.
- **OM02:** `ordinary_memory/effects.rs` visits only `SAssign`'s RHS expression.
  Its middle field is an assignment operation tag. `SGuard`'s first field is a
  literal byte budget; only its body is traversed. Legacy guards set the guard
  and root-bookkeeping resource flags. Compiler copied entry-error declarations
  survive specialization so legacy guard routing does not depend on removed
  template allocations. Tests cover nested guards, 1024/1048576/4294967296-byte
  budgets, both assignment tags, unrelated collecting operations, and malformed
  AST budgets (negative, beyond `usize`, Boolean), without executing forged IR.
- **OM03:** `ordinary_memory/provenance.rs` drops an `Err` payload's graph brand
  only for a resolved builtin `cgx2.core.Result` whose actual error argument is
  copied/scalar, or the authenticated scoped entry errors. User variants named
  `Err` and heap-bearing builtin Result errors retain provenance through matching,
  field access and returns. Invalid UTF-8-bearing sources reject with `CGT095`
  and exact original byte spans. Valid inspection within the region still works.
- **OM04:** rewrite and provenance analysis use the same local/qualified function
  resolution before interpreting `borrow` as capability borrowing. Local user
  functions execute normally; imported `util.borrow` coexists with genuine region
  borrowing. No public signature or exhaustive model enum was changed.

## Verification

All Cargo commands used `RUSTUP_TOOLCHAIN=1.98.1`, `--offline --locked`,
`CARGO_BUILD_JOBS=2`, private `target/release-ordinary-core-admission` and
`target/tmp-ordinary-core-admission`, `PYTHONDONTWRITEBYTECODE=1`, and the existing
`LIBRARY_PATH=/tmp/cgx2-wave22-lib`. rustc reports commit
`48a229ceaefd4985c50990b14116b6d856af0985`, LLVM 22.1.8.

| Check | Result |
| --- | --- |
| sema, CLI and package all-feature tests | 361 passed, 0 failed, 0 ignored |
| New focused Rust coverage (included above) | 12 integration tests and 1 private walker unit test passed |
| sema/CLI/package all-target/all-feature Clippy, `-D warnings` | passed |
| `cargo fmt --all --check`, `git diff --check` | passed |
| Workspace all-feature build for final canonical binaries/libraries | passed |
| New final source/package matrix | 196 assertions passed, 0 failed |
| Unchanged `ordinary-memory-evidence/run.py` (without optional regressions) | 117 passed, 0 failed |
| Unchanged default-managed main corpus | 369 passed, 0 failed |
| Unchanged managed repair subcorpus, invoked by default-managed runner | 678 passed, 0 failed |

The new matrix includes nine valid cases (six sources, three packages), each
checked/interpreted and executed as native debug, native release and standalone
emitted Rust, plus Python rejection/artifact preservation. Twelve invalid cases
cover all allocating markers, collection effect/capability requirements and
transitive calls, user heap-error escapes, and imported user functions retaining
`ReadArgs`/`ArgsRead`. CLI check/run diagnostics match all expected codes and byte
spans, including JSON Lines with multiple errors; native build/emission preserves
the first semantic diagnostic and existing artifacts.

Final binaries and canonical top-level probe libraries were frozen with the
unchanged `managed-core-repair-evidence/freeze.py` after all Cargo work. No Cargo
build ran during product verification. Final hashes are checked independently
and each product attempt records before/after hashes. Native release runs are
semantic checks, not performance measurements.

## Retained attempts and evidence

`ordinary-admission-repair-evidence/` contains bounded raw command records,
frozen hashes, toolchain identity, source hashes and gzip-preserved source,
standalone Rust, runner snapshots and raw product reports. `exports.json` maps
both original and stored SHA-256 digests; `verify.py` checks both forms and the
actual frozen binaries/probe libraries. The ignored private target retains the
executables and full working attempts. No evidence or scratch was deleted.
Final export verification passes for 339 exported files, 488 compiler inputs
and all 11 frozen binaries/probe libraries.

- Initial Rust baseline: 3 passed/8 failed. These include reproduced compiler
  defects and mistakes in new test inputs (Option versus scalar indexing and
  malformed-bound parsing assumptions). They are not eight independent product
  defects. Post-change attempts: 9/2, 10/1, then 11/0. The intermediate failures
  retain the real legacy-routing compatibility issue, a newline-inclusive span
  expectation, and the correction that a budget above `i64::MAX` can still be a
  valid literal `usize` legacy budget. Final expanded Rust suite passes above.
- Formatting's initial failed check is retained; only owned Rust files were
  formatted before the final passing check.
- New product attempts: 110/26 and 120/19 retain a fixture missing required
  `else` and incorrect `cgx2c` package argv. Corrected attempt 3 passes 145/0;
  expanded allocation-rejection matrix passes 177/0. An added imported user-effect
  case produced the correct four JSON Lines diagnostics and compiler first error,
  but its initial harness expected a JSON array/all human errors (191/5).
  The complete matrix corrects that output contract and passes 196/0. Sources,
  diagnostics, exits, failed standalone compiler attempts and artifacts are kept.
- The first private-branch creation was denied by the read-only Git common-dir
  mount (exit 128), then succeeded under the authorized sandbox escalation.
  The initial untracked `.serena/` configuration is outside the delivery and is
  left untouched. No validation attempt encountered a new `/tmp` quota failure.

`check.sh LABEL COMMAND...` records build/check commands with 1800-second wall,
1200-second per-process CPU, 8-GiB address-space, 512-MiB file and 2-MiB-per-stream
output bounds. Product children use 180-second wall, 120-second CPU, 4-GiB
address-space, 512-MiB file and 2-MiB-per-stream output bounds. These are per-process
limits and process-group cleanup, not aggregate containment. Product reports are
saved after each command/assertion; all failed attempts remain distinct.

## Open and unrun

OM05–OM07 runtime root/transfer/census defects and OM08 API visibility remain
owned by other repair lanes and are not claimed fixed. Passing existing corpora
does not negate their independently reproduced failures. Their original evidence
remains under `plans/release-completion/s2b-ordinary-memory-review/repros/`.

General aggregate ownership, source `copy_owned`/`promote` with failed-owner
continuation, complete authenticated v3 summaries and static dependency-file
attribution remain incomplete and assigned to a later sole producer. Optional
full historical regressions were not rerun here (coordinator scope). No full
workspace test claim, external leak audit, release performance measurement, or
broader release acceptance is made. Default-region policy remains Auto with
1-MiB trigger and 64-MiB limit; no runtime policy changed.
