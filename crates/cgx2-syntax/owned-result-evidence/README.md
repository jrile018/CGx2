# Owned Result / bounded no_gc delivery evidence

The executable bounded source deliverable is implemented. Full ordinary memory,
S2b and release completion remain **false**. The frozen contract and complete
remaining ledger are [S2B-OWNED-RESULT.md](../S2B-OWNED-RESULT.md).

Final offline locked workspace result: **1229 passed, 0 failed,
6 ignored in 265 suites**. Workspace
all-target/all-feature Clippy with `-D warnings`, fmt and diff passed.
The new product runner passed **584 checks** on the final complete run. The existing
ordinary-owned runner passed **256**; graph99, native guards518,
report layouts265 and all seven foundation runners passed
(159/278/207/118/88/31/104 checks respectively). Exact commands,
exits/counts/hashes, including earlier span-expectation failures and the final accepted_checks, are in [verification.json](evidence/verification.json).

The source product runs interpreter, native debug/release and independently
staged/compiled Rust. It covers real BudgetExceeded recovery, 39/40 owned-charge
boundaries, cumulative nonrefund, nested budgets and independent 0/31/32 guard
table boundaries, both arms, mixed copied/owned/borrowed/sum parameters,
if/early return, nested tail matches, depth-32 native/standalone nesting, discard and failure cleanup, source-order construction, exact
UTF-8 negatives, and artifact-preserving target/consumer rejection. The ordinary
user function named try_owned still runs through copied/native/Python paths.
Session/GuardBudgets/OwnedBytes hooks are **synthetic HostAllocation injection**;
native debug/release and standalone outcomes are compared, supplemented by
actual-source interpreter injection tests. No signal/crash counts as success.

Schema 1 compile_source/OwnedProgram/model/Evidence/Execution remain intact,
including exhaustive enums and old evidence struct literals/JSON. New APIs are
compile_source_v2, OwnedProgramV2, model_v2, EvidenceV2, ExecutionV2 and
emit_owned_result_rust. Both checked entry families use the same engine/runtime.
Conditional storage is initialized only on the Ok edge; Err carries no new owner.
Guard/session entry remains an uncaught, source-located CGR080 in this step.

Observed payload bytes are separate from actual runtime logical charges
(OwnedBytes capacity + 32 header bytes and guard-table capacity). Observation is
not RSS, foreign allocation, a general managed memory report or a leak proof.
Default-session collection counts stay zero in this owned-only profile.
Generic/recursive/package memory, default managed graphs/collections/roots,
named regions/transfers, broader control flow/reborrows, full recoverable entry,
cancellation/unwind/consumer closure and all R01-R15/S3/S4/external gates remain
mandatory. Leak-enabled acceptance and independent safety audit are unrun here.

No dependencies or manifests changed. **No root-lock reconciliation is needed**;
Cargo.lock remained identical to HEAD (SHA-256 `ea7678242cb40c8425c2b9875215f7024071c684cab59399d1d8a74a96750e3a`). Only cached local
dependencies were used. No installs, external spend, host security changes,
merge, publication or follow-on task occurred.

Reproduce from the enclosing commit in a writable private worktree:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-owned-result"
export TMPDIR="$PWD/target/tmp-owned-result"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
python3 crates/cgx2-syntax/owned-result-evidence/verify.py --output-dir "$TMPDIR/reproduction"
```

The product runner caps each process at 120 seconds wall / 60 CPU, 2 GiB address
space and 1 MiB retained output, and kills its process group on timeout. The
outer matrix caps commands at 900 seconds wall / CPU, 4 GiB address space and
8 MiB logs, kills process groups, and uses two Cargo/linker threads. Per-process
limits are not a claim of aggregate cgroup confinement. Preliminary failures
and environment limitations are retained separately and described in the JSON.
Raw final logs are exported as `.log.gz` with original and gzip SHA-256 values;
`gzip -dc FILE.log.gz` recovers the exact bytes. The enclosing Git commit is the
delivery identity; source/runtime/file hashes bind the verified content.
