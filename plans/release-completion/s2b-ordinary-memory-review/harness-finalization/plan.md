# Ordinary-memory evidence finalization repair

## Bounded objective and starting contract

Repair the remaining production-validator acceptance holes found in harness delivery d43eb9fa43da6c19385f2c54195de506193e143d. The coordinator stages that truthful but incomplete delivery as a repair base; this is not ordinary-memory or release acceptance. Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, the release-completion README/contracts/worker-rules/dispatch, this plan and the ordinary-memory review. Use MegaMemory overview/query/record and Serena instructions.

## Exclusive ownership

- Python harness files under crates/cgx2-syntax/ordinary-parity-repair-evidence/.
- New evidence only under that directory's finalization-evidence/.
- New crates/cgx2-syntax/S2B-ORDINARY-HARNESS-FINALIZATION.md.

All existing evidence/ and hardening-evidence/ files, prior reports, runtime_probe.rs, fixtures, implementation files, historical harnesses/evidence, root manifests/lock and coordinator plans/ledgers are read-only. Do not modify a language diagnostic or expected runtime result to make evidence pass. No follow-on dispatch, merge, release, installs, paid API calls, external contact or host-security changes.

## Confirmed defects and required behavior

1. validation.sentinel_status currently accepts exit1 plus an expected-code substring and unchanged sentinel bytes. Duplicate CGE006 lines, CGXC005 followed by CGE006, and CGXC005 merely mentioning CGE006 all pass. Parse the actual command's complete diagnostic result; require exactly the expected diagnostic under the command's real output format, reject duplicates, unrelated errors, malformed/missing observations and abnormal execution, and independently require exact preserved sentinel bytes. Main build/emit and package Python rejection callers must exercise the same production validation. A separate successful `check` invocation cannot validate the later build/emission response.
2. inventory_errors currently rejects only pending/exception/missing reasons. Timeout, signal and overflow command rows can pass finalization. Require valid completed command structure and normal termination for every required command. Explicit expected nonzero product rejections and retained genuine product failures remain distinguishable from abnormal execution; do not require all legitimate commands to exit0. Add explicit normal successful toolchain identity/sysroot checks so failed required setup cannot leave a complete accepting report. Propagate failures through Journal.finish to a nonzero result and incomplete status, retaining all partial observations and failed prerequisite dispositions.
3. Journal currently hardcodes reviewed_base89464a7 and freeze.py requires HEAD equal it. Record the explicit current candidate/build source commit, dirty/source snapshot and harness identity. Verify input hashes against the exact recorded build. Fail closed on missing/stale identity; never relabel a candidate by changing a constant, infer matching compiler bytes merely from a nearby commit, or count an old candidate's checks as new-source validation. Permit a new harness revision to verify an unchanged compiler candidate only with explicit, independently auditable source/harness identities.

## Verification and retained evidence

Use a private target and TMPDIR, Rust1.98.1, CARGO_BUILD_JOBS=2, all Cargo offline/locked, PYTHONDONTWRITEBYTECODE=1. Preserve all failed attempts in unique directories. Do not overwrite earlier frozen dependencies, run Cargo during frozen product checks, or remove another task's scratch. No performance measurements.

Run all existing 58 selftests (48 primary, 2 supplemental, 8 historical) unchanged, plus meaningful tests against production helpers and Journal.finish. Cover duplicate and unrelated/mixed diagnostics, expected code only in prose, malformed/missing streams, wrong exit/bool exit, timeout/signal/overflow/invocation exception, missing or failed toolchain observation, valid expected nonzero rejection, valid successful command, and source identity mismatch. Demonstrate old-validator failures with identical new adversarial inputs. Retain actual counts and original failed attempts.

Revalidate retained hardening observations under repaired validators into a new audit, never rewrite history. Distinguish validator vulnerability from any actual invalid retained outcome. Obtain or consume an exact coordinator-verified compiler candidate only after matching all source/build/toolchain inputs. Run fresh main, Auto, package and OM01–08 review matrices with complete inventories, plus unchanged historical corpus and all18 regressions. Use actual external standalone cwd. Preserve static dependency filename and any surviving recovery/historical failures as red; reviewed expectation reconciliations remain separate with actual source/trace evidence. No dropped checks or normalization of root counts, charges, error codes or spans.

If rebuilding is necessary, run workspace all-feature tests, all-target/all-feature Clippy -D warnings, fmt/diff checks and build, recording every result. A supplied coordinator build may be reused only by exact source/toolchain/hash identity. Freeze all7 binaries,5 canonical probe libraries, consumed dependencies, compiler/runtime/manifests and toolchain/harness inputs, audit before/after, preserve source/executable hashes and invocation outcomes. Store fresh compressed evidence and a countable manifest only under finalization-evidence/.

## Completion

Report changed files, exact candidate/build/harness identities, checks/counts, retained failures and limitations. Commit only owned paths and stop for review. This closes only the bounded harness defects when independently accepted; R01–R15, general aggregate ownership, source copy_owned/promote, authenticated v3, static dependency diagnostics and external acceptance remain required. Do not claim project completion.

## Coordinator-supplied candidate

The compiler candidate was built from ded3768480c745ce544d33c56aea44d4fc0bfa5a. Raw build and product evidence is under /home/john-riley/projects/lang/target/ordinary-finalization-combined-20261024; the frozen binaries are verification/frozen and the inventory is verification/inputs.json. coordinator-invocation.json records the actual commit, clean initial worktree and all flags, including CARGO_INCREMENTAL=0 and dev/test debug=0. Workspace1383/0/6, Clippy, fmt and build pass. Final combined results and immutable archive are recorded under plans/release-completion/s2b-ordinary-memory-review/evidence/finalization-combined.

The new chat base additionally includes coordinator review/plan/evidence updates and a one-line recovery-runner compatibility fix after the compiler commit; compiler implementation is unchanged. The recovery helper now accepts exactly one old four-root or already-corrected six-root formula before running all unchanged budget controls; its original failed attempt remains retained. Verify this and every compiler input by relative path and bytes, plus toolchain/artifact/dependency identity, before reusing this build. Preserve the existing frozen tree and its manifests. Your new harness snapshot and candidate manifest must identify both the original compiler build and your actual harness revision; do not mutate prior manifests to claim your checkout produced these binaries. Any alternate frozen view must preserve and verify source identity and dependency membership. Use a fresh output directory for every repaired-harness product run. Existing coordinator reproduction inputs are retained under repros/harness-finalization.
