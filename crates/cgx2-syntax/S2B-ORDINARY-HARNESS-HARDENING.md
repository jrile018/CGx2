# CGx2 ordinary-memory harness hardening

Bounded repair delivered for review on `riley/ordinary-harness-hardening`.
**Ordinary memory and release remain unaccepted.** Compiler/runtime implementation,
other lanes, root Cargo manifests/lock, coordinator documents and original evidence
were not changed.

Source base: `89464a76c9bd84a3b090ec58a14ca6be0043c6ea`.
The delivery commit identifier is supplied in the final review message; this report
belongs to that commit.

## Change and acceptance semantics

`ordinary-parity-repair-evidence/validation.py` requires actual nonnegative
Python/JSON integers for `baseline_roots`, `final_roots`,
`roots_registered`, `roots_released`, and `cleanup_failures`. Booleans,
nulls, strings, floats, negatives and missing fields fail. Both censuses must be
zero, registration/release counts equal, and cleanup failures zero. No counters
are filled in. Only the explicitly expected reserve-1 Session-entry rows accept
the exact `session_entered: false, metrics: null` observation.

Direct-probe JSON must have correctly typed `name`, strict boolean `passed`,
and string `details`. Fixed cases are declared independently of received rows.
The explicit baseline reserve-attempt count and bound row must agree and be in
1..64. Required reserve indices include the first unaffected control. Empty,
malformed, duplicate, substituted or omitted rows fail.

Main, Auto and package inventories require every check/interpreter/native debug/
native release/standalone route and its result, cleanup and parity evidence.
Standalone routes also require emission, compilation and execution. All 64
interpreter reserve indices and all three compiled routes through
`last_affected + 1` are required. Skipping compiled faults remains non-accepting.
Duplicate command/check identities are rejected before dispatch/insertion and
again at finalization. Missing downstream work after a failed prerequisite makes
the run incomplete and nonzero. Actual product failures remain failed even when
coverage is complete. Incremental pending invocations, exception records and
atomic report persistence remain intact.

`integrity.py` records recursive dependency membership, bytes, hashes, inode/mode,
and symlink target identities. Each product suite verifies the complete frozen
inventory before and after execution. It detects additions, deletions, changed
bytes, same-byte replacement and changed symlink targets. Command records also
hash the executable actually invoked. Sentinel checks require the expected
diagnostic/exit and exact pre-existing bytes, with missing files failing closed.

`verify.py` respects the private target/TMPDIR. `freeze.py` finalizes the same
already-built canonical candidate without Cargo after the final harness edits.
`replay.py` runs the unchanged historical runner and its 18 regressions under a
real outside-repository directory, retaining exact reports/sources and executable
hashes. `quality.py` checks the original evidence inventory. The README documents
the updated commands, strict semantics and accounting.

## Frozen candidate and environment

All Cargo dependency access was offline/locked with Rust 1.98.1,
`CARGO_BUILD_JOBS=2`, `PYTHONDONTWRITEBYTECODE=1`, and
`LIBRARY_PATH=/tmp/cgx2-wave22-lib`.

- Target: `target/release-ordinary-harness-hardening`.
- TMPDIR: `target/tmp-ordinary-harness-hardening`.
- Final candidate: `target/tmp-ordinary-harness-hardening/freeze-1/frozen`.
- Actual external standalone root:
  `/home/john-riley/.codex/visualizations/2026/10/24/01a16ea8-bbf7-7381-8599-5953d15d1757/ordinary-harness-hardening`.

The final inventory contains seven consumer executables, five canonical top-level
probe libraries (CLI, sema, package, Rust emitter, runtime), and the complete private
dependency directory. It has 1,368 artifact files, 214 compiler/runtime/manifest
inputs, 35 harness/fixture inputs and 68 toolchain/linker-library inputs.
Canonical libraries were never selected from arbitrary hashed dependencies.
The private dependency directory resolves to the recorded private build directory;
both its membership and each byte/identity are checked.

Frozen manifest SHA-256:
`265cc7b0f69d8687b4cb0a7a7517b549e9326172207ee00c17193f9110b893a3`.

Full input inventory SHA-256:
`8b55b4378d86e26cf5862e12c82ec8fd7350667c8f9da7d0a8788d882f9ab8b3`.

One explicit final workspace build followed the local tests, Clippy and formatting.
No Cargo ran during product verification. Finalization reloads the final harness;
the earlier verification launch versions and the two subsequently changed helper
lines are retained separately in `verification-1/launch-snapshot`, with provenance.
Compiler/runtime sources did not change.

Standalone Rust emission, rustc compilation and execution used real outside-Git
directories, verified by actual `git rev-parse` rejection. Emitted sources and run
executables have recorded byte hashes. Standalone rustc argv contains no project
rlib or dependency search path. Existing supervision remains 180 seconds wall,
120 CPU seconds, 4 GiB address space, 512 MiB file size, and 2 MiB per output
stream; suite bounds remain separately recorded. Timing data is supervision
metadata, not performance evidence.

## Actual verification

| Check | Actual outcome |
| --- | --- |
| Local all-feature workspace tests, offline/locked, no-fail-fast | **1,326 passed, 34 failed, 6 ignored; exit 101** |
| Workspace all-target/all-feature Clippy, warnings denied | Passed |
| Workspace formatting | Passed |
| Workspace all-feature offline/locked build | Passed |
| Main lane selftests | **48 passed: 21 retained + 27 added** |
| Supplemental explicit prerequisite/sentinel selftests | **2 passed** |
| Unchanged historical harness selftests | **8 passed** |
| Total Python selftests | **58 passed; 29 newly added** |
| Syntax and diff checks | Passed |
| Final main matrix | **971 passed, 11 failed; complete; exit 1** |
| Direct public-runtime subset | **50 passed, 0 failed** |
| Forced Auto suite | **32 passed, 0 failed** |
| Actual package value/error suite | **43 passed, 0 failed** |
| Unchanged historical product | **136 passed, 0 failed; all 18 regressions** |
| Historical outer coverage/integrity checks | **9 passed** |
| OM01–OM08 review replay | **19 passed, 0 failed** |
| Quality report | **9 passed** |
| Independent final audit | **25 passed, 0 failed** |
| Original evidence preservation | **All 393 file hashes identical** |
| Export audit | **332 primary archives + 30 exact compressed inputs verified** |

The main matrix includes all 64 source reserve indices, direct reserve indices
1..17 with explicit baseline bound 16, and 66 compiled fault runs covering indices
1..22 across debug/release/standalone. The source's last affected index is 21.
All required rows are present and unique. The final audit checks all 14 physical
sentinels, complete inventories, final input identities and 808 owned command
stream hash/byte observations. The supplemental tests explicitly retain a failed
build followed by absent execution, and replace a sentinel while presenting the
correct diagnostic. All destructive selftests use disposable copies.

The local 34 Rust failures are 10 interop-worker transport failures reporting
`child lifecycle guard/exec: Permission denied (os error 13)` and 24 notebook HTTP
failures reporting permission denial. Their raw outcomes remain failed. The
coordinator's separately retained 1,360/0/6 result is not relabeled as this local
run. No host security setting or permission workaround was applied to those
tests, and they were not retried to obtain a pass.

## Copy-budget correction and remaining product failures

Only the owned direct probe's expectation/comment changed. The required temporary
source protection survives destination publication. Publication sees len1/cap1
and charges a complete replacement cap2 root table without refund, adding
`2*root` to the previous formula:

`6*root + 3*(size_of::<(Handle,u32)>()+1) + 12+8+32 + alloc = 439`.

On the pinned 64-bit toolchain, root=32 and alloc=72. Guard metadata adds 16,
giving the observed **455** total. Zero, exact439 and one-under438 all pass,
including source validity, destination failure atomicity, balanced roots and
nonrefundable charging. This does not waive a runtime charge. The original
375-byte failure remains in the read-only combined-review matrix at
`plans/release-completion/s2b-ordinary-memory-review/evidence/combined-reviewed/combined/matrix-retry/report.json.gz`;
the new raw result and probe snapshot are in the fresh matrix export.

The 11 remaining failed main assertions are:

| Actual defect | Failed assertions | Raw observation |
| --- | ---: | --- |
| Static dependency filename attribution | 1 | CGT095 span223..225 reports main.cg; lib.cg remains required |
| Reserve-10 recovery | 5 | CGR090 HostAllocation at 0..1306 instead of recovered scalar outcome |
| Reserve-11 recovery | 5 | CGR090 HostAllocation at 661..663 instead of recovered scalar outcome |

Each reserve failure is two interpreter assertions plus one result assertion per
compiled route. Agreement on the actual erroneous primary code/span is retained
separately from semantic recovery acceptance. Index21 now passes on this base.
No raw error, span, counter or charged byte was normalized. The prior stale copy
and coordinator-manifest failures remain historical, while this candidate's
canonical seven/five manifest and derived expectation pass.

## Evidence and attempt retention

New evidence is confined to `ordinary-parity-repair-evidence/hardening-evidence/`.
`run-2/manifest.json` hashes 332 compressed reports and source snapshots;
`summary.json` records every attempt. `compressed-inputs-manifest.json` additionally
hashes 30 byte-identical already-compressed historical replay sources/inputs.
`archive-audit.json` verifies both sets. Large rebuildable binaries/libraries are
omitted from Git while their complete byte and identity inventories are retained.

The first export stopped on missing argv metadata in manually retained sandbox
startup records. Its traceback, five partial files in `run-1/`, and their hashes
remain preserved. Each original startup report is retained separately; adding
the exact exec_command tool input did not change its diagnostic or failed
disposition. The successful retry writes only `run-2/`. Exporter and frozen
product inputs did not change.

Four sandbox startup failures with `/tmp/.git: Quota exceeded (os error 122)`
remain distinct non-accepting attempts. After automatic review approved the
authorized launches, Auto/history/package/review/quality ran outside the broken
sandbox setup with the same frozen bytes and existing harness resource limits.
No host settings changed. Initial local branch/ref inspection diagnostics and the
successful reviewed branch creation are retained; an audit corrects the inferred
subcommand exit annotation where a composite shell command had masked it.

The historical owned-Result runner's original nonzero result and superseded
shadow-builtin negative remain present. Reconciliation is separate, and its
117 bounded assertions plus 19 regression/reconciliation assertions still total136.
All 18 regression commands use the same canonical frozen candidate. Partial
attempts, expected negatives, genuine failures, exception text, argv/cwd,
environment, exit/signal/reason, sources and stream hashes remain reviewable.

## Limits and review status

This completes the assigned evidence-harness repair for review; it does not
accept ordinary memory. The nominal constructor-borrow follow-up and reserve
recovery producer remain separate. The small OM04 review exercises the existing
user-function case and does not close the nominal-constructor finding.

General aggregate ownership, projections/partial moves/reborrows, recursive/
generic/package ownership, source copy_owned/promote with same-owner failure
continuation, full authenticated v3 summaries and static dependency attribution
remain open. Identifier exhaustion, global allocator OOM, abort/process-kill,
forgotten scopes, leak-enabled assurance, independent audit, external runtime/
isolation dependencies and all broader R01–R15 release gates remain unrun or
incomplete as previously specified. Pause histograms and only the session
component of region identity remain excluded from deterministic comparisons.

No other worker's changes were adopted. No delegation, other-chat messaging,
merge, publication, package installation, external contact/spending, performance
measurement or follow-on task was performed.
