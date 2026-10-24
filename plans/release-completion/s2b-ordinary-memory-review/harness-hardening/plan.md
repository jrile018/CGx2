# Ordinary-memory parity harness hardening

## Bounded objective

Harden the existing evidence-only parity harness so missing or malformed evidence cannot satisfy an acceptance check. Retain every genuine compiler/runtime failure. Deliver corrected harness code, meaningful adversarial selftests, and a fresh run against one frozen combined candidate. This task does not repair language, ownership, runtime, package, or API implementation behavior.

Start from the exact combined base supplied by the coordinator in the attached worktree; record its commit identifier. Do not assume the previous worker's frozen binaries contain subsequent repairs. Read the applicable AGENTS.md, ordinary-memory review ledger, evidence-parity plan, prior repair report, and new assignment before work. Follow MegaMemory overview/query/record and Serena instructions.

## Exclusive ownership and immutable evidence

- Existing `crates/cgx2-syntax/ordinary-parity-repair-evidence/**`, subject to the immutability rule below.
- New `crates/cgx2-syntax/S2B-ORDINARY-HARNESS-HARDENING.md`.

All already-retained evidence beneath `ordinary-parity-repair-evidence/evidence/**` is immutable, including manifests, summaries, snapshots, commands, failures, and compressed artifacts. Do not regenerate or replace it. Put all newly committed evidence beneath a distinct `ordinary-parity-repair-evidence/hardening-evidence/**` directory, with a new manifest and summary. Every scratch run and failed attempt must have a distinct, non-overwriting directory.

The prior `S2B-ORDINARY-PARITY-REPAIR.md`, all historical runner/evidence directories outside this lane, root Cargo manifests/lock, coordinator plans/ledgers, and all implementation files are read-only. In particular, do not edit `crates/cgx2-sema/src/ordinary_memory/mod.rs`, `provenance.rs`, or the borrowed-nominal follow-up's new tests/evidence paths. No merge, follow-on task, delegation, other-chat messaging, package/tool installation, external contact/spending, or host security changes.

## Required changes

### 1. Validate root observations strictly

Replace the permissive `roots_ok` predicate with a shared validated interpretation used by the main matrix, package, Auto, and review harnesses.

- Require all five fields: `baseline_roots`, `final_roots`, `roots_registered`, `roots_released`, and `cleanup_failures`.
- Each must be an actual nonnegative JSON/Python integer. In Python, reject booleans explicitly (`type(value) is int`); also reject null, strings, floats, negative values, and missing fields.
- Require `baseline_roots == final_roots == 0`, `roots_registered == roots_released`, and `cleanup_failures == 0` for rows whose contract requires full cleanup.
- A valid observation with zero final roots and unequal registration/release counts must fail. Missing counters must never compare equal through `None == None`.
- Keep pre-Session failure separate: only the specifically expected entry-failure row may satisfy `session_entered: false, metrics: null`. Do not manufacture zero counters or accept unavailable metrics in rows that entered a Session.
- Retain actual raw observations and a useful failure reason; do not silently fill defaults.

### 2. Validate required evidence coverage

Define the expected case/target inventory from fixture specifications and the explicit bounded sweep contract, independently of whatever rows happen to be emitted. Audit that inventory before declaring a run complete.

- The direct runtime probe must emit exactly one row for each required named budget, cleanup, copy, Session-entry, and sweep case. Validate row structure and a strict boolean `passed` field. Successful empty stdout, malformed JSON, duplicate names, unknown substituted names, and omitted cases must fail the harness.
- Distinguish the direct probe's fixed cases from its dynamic reserve-index rows. Derive the sweep inventory from its explicit baseline reserve-attempt bound, validate the allowed bound, and require every index through the first unaffected control. Do not infer completeness from the rows received.
- Require each source fixture's check/interpreter and native debug, native release, and real standalone emit/build/run/result/error/cleanup/parity evidence, according to that fixture's declared outcome.
- Require all 64 interpreter reserve indices in the existing source sweep. Validate the affected bound and unaffected tail, and require all three compiled routes for indices `1..=last_affected + 1` as presently specified. Preserve the distinction between recovered scalar outcome and agreement on an actual primary error.
- Apply explicit coverage checks to the package and Auto suites as well. Required targets cannot disappear because a collection was empty or a branch skipped execution.
- A failed prerequisite may leave downstream work explicitly failed, blocked, or unrun with a reason; it must make the run incomplete/nonzero. Do not count missing downstream rows as successful evidence. Intentional exploration options such as `--skip-fault-native` must remain non-accepting.
- Reject duplicate command/check identities in the required inventory rather than overwriting or double-counting them. Keep existing incremental command persistence and exception records.

### 3. Freeze and verify the complete executable input set

Use one frozen combined candidate throughout final product verification. Extend integrity checks to include the dependency files actually consumed by canonical-library probes, not only top-level binaries and rlibs.

- Record seven consumer binaries and the five canonical top-level probe libraries: CLI, sema, package, Rust emitter, and runtime. Never select an arbitrary hashed library as the canonical probe library.
- Record the frozen dependency directory's file membership and byte hashes, plus compiler input files, root manifests/lock, runtime sources, toolchain identities, and harness/fixture snapshots.
- Verify this inventory before and after verification. Changed, missing, replaced, or newly introduced frozen dependency/artifact inputs must fail explicitly. Resolve/copy inputs consistently so a changed symlink target cannot evade the byte identity check.
- Do not run Cargo or otherwise replace compiler inputs while final frozen product checks are running. Preserve hashes of emitted standalone source and each executable actually run.
- Sentinel checks must validate both the expected rejection and preservation of the exact pre-existing bytes. An unrelated error, missing sentinel, or changed sentinel must fail.

### 4. Reconcile the reviewed copy-budget bookkeeping change

The combined runtime repair now keeps the temporary source root until copy publication. The one-node direct probe's old `4*root` formula assumes publication reuses a released source slot. Publication now sees len1/cap1 and reserves a replacement cap2 table, charged in full without refund: add `2*root`. With pinned Rust1.98.1 on this64-bit host, the formula is `6*root + 3*(size_of::<(Handle,u32)>()+1) + 12+8+32 + alloc = 439`; guard metadata16 makes455 observed total. This is a legitimate additional bookkeeping charge required by the lifetime repair, not a promise of a fixed375-byte copy budget.

Coordinator independently ran a disposable one-line formula-only probe against unchanged combined frozen libraries: all50 direct assertions pass, including zero/exact439/one-under438, source validity, failure atomicity, and balanced roots. Original375 failure remains retained in the combined matrix. Correct only this expectation/comment in the owned probe, retain derivation and both attempts, and continue checking full nonrefundable charges and all boundaries. Do not adapt expectations to arbitrary outputs or waive remaining recovery/diagnostic failures. Evidence is in the coordinator combined-review export; if a later runtime revision legitimately changes bookkeeping, derive and report the exact new charge before changing its assertion.

### 5. Add meaningful adversarial selftests

Retain and run all existing 21 lane selftests and 8 unchanged historical harness selftests. Add selftests that exercise the production validators/report finalization, not copies of their comparisons.

Required negative cases include:

1. Every missing root field; null, bool, string, float, and negative counters; unbalanced counts with zero census; nonzero cleanup failures; and a valid all-integer control.
2. Empty, missing, malformed, duplicate, or incorrectly typed direct-probe rows; a valid complete direct inventory; an omitted reserve index.
3. Missing or duplicate interpreter, native debug, native release, and standalone rows; a failed build followed by absent execution; explicit exploration skip; a valid complete target inventory.
4. Mutation/deletion/replacement of a frozen top-level artifact and of a consumed dependency, and dependency membership changes. These must fail even if other outputs appear successful.
5. A replaced, missing, or modified output sentinel despite a seemingly correct diagnostic; wrong diagnostic/exit and valid preserved-sentinel controls.
6. Timeout, signal, overflow, invocation exception, duplicate diagnostics, and missing observations cannot satisfy expected-negative/product checks. Keep partial evidence through simulated quota/spawn exceptions.

All destructive selftest mutations must affect disposable temporary copies, never the actual frozen candidate, historical evidence, or repository inputs. Record the exact new test count and outcomes. Do not weaken existing tests to obtain a pass.

## Verification environment and sequence

Use the attached worktree and a private target/scratch pair, for example:

```
RUSTUP_TOOLCHAIN=1.98.1
CARGO_BUILD_JOBS=2
PYTHONDONTWRITEBYTECODE=1
CARGO_TARGET_DIR=$PWD/target/release-ordinary-harness-hardening
TMPDIR=$PWD/target/tmp-ordinary-harness-hardening
LIBRARY_PATH=/tmp/cgx2-wave22-lib
```

Create these private directories. All Cargo dependency access must be offline and locked. Parameterize existing helper paths where needed within owned files so a hardening run cannot overwrite prior lane scratch/evidence. Do not install packages or change permissions/security settings to evade a failed check.

1. Record the exact combined source base, dirty state, owned-path diff, and pre-existing evidence hashes.
2. Run new and existing Python selftests, Python syntax checks without repository bytecode writes, and `git diff --check`.
3. Obtain final binaries from one explicitly recorded combined candidate build in the private target, using `cargo build --workspace --all-features --offline --locked`. Record any coordinator-provided workspace/Clippy/fmt verification by exact source identity, command, and raw-report hash; do not claim checks that were not run or reuse incompatible source results. If final artifacts are supplied already frozen by the coordinator, validate their exact source/toolchain identity instead of rebuilding them.
4. Freeze the seven binaries, canonical libraries, complete consumed dependency inventory, compiler/runtime inputs, and toolchain hashes. After this point, no Cargo during product verification.
5. Against that same frozen candidate, run the hardened main matrix, forced-Auto suite, actual package value/error suite, and the unchanged historical ordinary-memory corpus with all 18 regressions and historical-negative reconciliation. The unchanged corpus expectation remains 117 bounded assertions plus 19 regression/reconciliation assertions = 136; retain its original runner and fixtures unchanged.
6. Run the existing OM01–OM08 review replay against the same final matrix where supported, retaining its actual remaining failures. Do not use old baseline rows as evidence about the combined candidate.
7. Audit required coverage, all byte hashes, sentinels, source snapshots, and compiler/runtime/manifests after verification. Export fresh reports and sources only to the new hardening evidence directory.

Standalone emit/build/run must use a real directory outside every Git repository, under an authorized task-private location. Verify outside status and retain cwd/argv. The emitted Rust must compile with the installed toolchain and no project rlib/dependency search paths; run the actual relocated executable there. Preserve resource limits and bounded output using the existing supervisor. Timing fields are supervision data, not performance evidence.

Every attempt must retain command argv/cwd/environment, exit/signal/reason, bounded stdout/stderr with byte hashes, inputs, toolchain/runtime/compiler identities, and partial reports written before dispatch and after commands/checks. Preserve every nonzero outcome. Never rename a genuine product failure into a passing negative, drop a failed row, or normalize away charges, root counters, collection counts, error codes, or byte spans. The existing pause-histogram and region-session-identity exclusions remain narrowly documented.

## Concrete acceptance and reporting

The harness-hardening deliverable is reviewable when:

- All strict validation and required-coverage adversarial tests pass; a missing/duplicate/malformed required row, changed input, or changed sentinel demonstrably produces a nonzero/incomplete result.
- The final main/Auto/package/corpus runs all use the same verified frozen combined candidate and include every required row or an explicit non-accepting disposition.
- Raw reports, independently countable assertions, archive hashes, exact command outcomes, and immutable old evidence agree. No implementation or other lane changed.
- Genuine remaining product failures are still red and separately categorized. Report the new actual counts; do not require or reproduce the old `902/75`, `40/0`, `29/0`, or `136/0` by changing expectations.
- The new report names all failures/unrun checks, limitations, exact base/frozen identities, files changed, verification performed, and the final commit identifier.

The prior 75 matrix failures were 1 OM06 copy assertion, 58 OM07 root-accounting assertions, 1 static dependency-file attribution assertion, and 15 reserve-recovery assertions. The last group comprises five assertions each for reserve indices 10, 11, and 21, whose old actual errors were `CGR090 HostAllocation` at `0..1306`, `661..663`, and `790..792`. These are historical observations, not required future ordinal mappings or current defect counts. Recompute after the combined fixes; preserve any surviving or newly exposed errors without weakening scalar recovery expectations. Static dependency attribution remains a known producer limitation unless independently fixed in the coordinator's candidate.

A completely executed red product matrix may support accepting the bounded harness correction; it does not mean the product passes. Explicitly distinguish harness correctness/coverage from product acceptance in the report and status fields.

Do not close full ordinary memory, borrowed-nominal producer work, general aggregate ownership, source `copy_owned`/`promote` with same-owner failure continuation, authenticated v3 completeness, precise dependency-file diagnostics, external assurance, or release gates. Do not rerun blocked external gates or claim process-kill/global-OOM/identifier-exhaustion guarantees. Record the work in MegaMemory, commit only owned changes in the attached worktree, and stop for coordinator/user review.
