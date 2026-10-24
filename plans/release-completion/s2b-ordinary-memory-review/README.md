# Ordinary-memory delivery review: repair base only

Delivery a3b98d7374de8aea90a610a73c99b04deb8857a1 is clean and stopped. It is staged as 32032fd3c34d1a1f71e5229ec3e72dcbf0c462a6 on the coordinator branch solely for repairs. **Not accepted; no S3 or new language consumers.** Main remains unmerged.

Independent source/runtime/consumer reviews and coordinator frozen-binary reproductions confirm:

| ID | Priority | Defect | Repair owner |
| --- | --- | --- | --- |
| OM01 | P1 | Shadowed managed.ref/managed.collect allow real region/collection under effects() caps() | core-admission |
| OM02 | P1 | Legacy no_gc budget traversed as expression index panics; assignment tags likewise misread | core-admission |
| OM03 | P1 | User variant named Err erases heap payload region provenance, accepted source escapes at runtime | core-admission |
| OM04 | P2 | Legitimate user borrow function intercepted as region capability intrinsic | core-admission |
| OM05 | P2 | Region arm assignment to outer managed binding releases registration through wrong root table: NotRooted | runtime-roots |
| OM06 | P2 | Safe runtime copy_graph_to_v1 releases temporary source root before same-region allocation; Auto trigger1 invalidates source | runtime-roots |
| OM07 | P2 | Copy publication root absent from evidence registrations: copy-cycle 25 registered / 26 released | runtime-roots |
| OM08 | P2 | Ordinary api routes to memory-effects view, exposing private functions and losing public records | api-visibility |

Reproductions under repros/ retain exact source/commands/exits/hashes, including unsuccessful coordinator package argv followed by correct positional package invocation. OM06 is direct safe-runtime API exposure; source evaluator currently roots its expression arguments. Do not claim a demonstrated source-level OM06 failure.

## Evidence and remaining scope

Worker evidence independently verified: 209 exported hashes, 205 compiler inputs, frozen binaries/libraries, 1327 Rust tests/0 failed/6 ignored, 117 new product assertions plus 19 reused regression assertions =136, and22 independent standalone checks. Parent product attempt retains6 package-argv failures; final run fixes invocation and reuses only rows against identical frozen artifacts. Initial exploratory interface-sema raw diagnostics were overwritten; first scratch cleanup audit failed; final artifacts do not recover those missing histories.

Coordinator rebuild independently passes1327 workspace tests/0 failed/6 ignored, Clippy, fmt/diff and8 harness selftests. Full product/standalone replay disposition is recorded separately when complete. These successes do not negate the defects or missing acceptance rows.

Delivered support includes scoped named regions/guards, contextual allocation, managed graph copy, scalar pre-Session fallback and source bundles. Required general aggregate ownership (projections/partial moves/reborrows/recursion/generics/packages), source copy_owned/promote with allocation-free same-owner failure, complete v3 summary/body authentication, precise static dependency-file attribution and full fault/cleanup/parity remain incomplete. v3 currently adapts to v2. R01–R15 stay required without narrowing.

## Exclusive repair frontier

Four disjoint plans share this reviewed repair base: core-admission, runtime-roots, api-visibility, evidence-parity. They repair existing reviewed surfaces; none adopts unfinished ownership interfaces. The evidence lane records known implementation failures accurately and can be integrated before the combined post-repair rerun. Do not cherry-pick across workers or edit another lane. Coordinator reconciles and runs full integration gates before declaring a new accepted checkpoint or next sole producer.

Independent plan review confirmed disjoint feasible lanes after assigning sema/src/effects.rs to core and freezing authenticated original-provenance API projection. Package dependency-file diagnostics remain explicitly deferred/failed; no impossible acceptance promise was moved into API repair.

Coordinator full product replay passed136/0, including unchanged current regressions and explicit historical reconciliation. Initial standalone attempt failed on /tmp quota before final report persistence; traceback preserved, exact own inactive scratch archived and hash-verified before removal (26,017,239 bytes), no other directory removed. Partial command records were in memory and are unavailable. A retry changes only scratch-directory placement to a private location outside every repository; actual outcome follows in retained evidence. This motivates incremental harness reporting in evidence-parity.

Standalone directory-only retry passed22/0; frozen binaries/libraries remain unchanged. Independent baseline:1327Rust/0failed/6ignored,8harness selftests,136product assertions and22standalone assertions. See evidence/review-status.json and compressed raw reports. This establishes a buildable reviewed repair base, not accepted ordinary memory.

## Admission delivery reviewed; nominal follow-up required

Core9330300 is staged1d33177 only as an incomplete repair base. Independent review confirms original OM01–OM03 and user-function borrowing fixes; coordinator361Rust/196product and quality gates pass. OM04 still rejects a valid record constructor named borrow with CGT095 (repros/core-nominal/), so full lane acceptance is held. Every core standalone command used a target/.../outside directory inside its checkout: retain as emitted-Rust parity, not external-cwd acceptance. The narrow borrow-nominal plan fixes resolution and requires unchanged196 matrix replay from an actual outside-repository output directory. API repair is already accepted; runtime/evidence review outcomes follow below.

## Four repairs reviewed; three bounded follow-ups

All four chats are stopped and reviewed. Runtime f255dfe1 is accepted/integrated da48284 for OM05–07; API21452c2 remains accepted/integrated64a71e7. Core9330300 is reviewed incomplete for nominal `borrow`, and evidence80bd037 is staged0acc327 as truthful incomplete evidence. No broad memory or S3 acceptance.

Independent runtime review verifies39 owned hashes,144 archives,205 compiler inputs,9 runtime inputs,66 dependencies and12 artifacts; outer assignment roots3/3, copy-cycle26/26 and Auto trigger1 safe-copy source survival pass. Evidence review verifies390 archives and4980 stream hash/byte pairs; all actual root observations have integer counters, so the old902/75 baseline is truthful despite a permissive validator. Missing-field root validation, missing direct/target row detection, and artifact/sentinel adversarial selftests require hardening.

Combined source0acc327 passes1360Rust/0failed/6ignored and workspace Clippy/fmt/diff/build. The first workspace attempt failed the fake solver protocol test (1302passed/1failed/6ignored); its original parser omitted the failing suite in summary counts, and the corrected raw-log totals are retained in coordinator-audit.json. An isolated retry passed before the complete repeat; no live solver evidence is implied.

The unchanged API84, admission196 (now actual outside-repository cwd), Auto29, and21 new harness selftests pass. Main combined raw matrix965passed/13failed:10 reserve-recovery assertions at indices10/11; static dependency filename; stale copy budget; coordinator supplied-manifest mismatch. Runtime index21 now passes. The old59 root/lifetime failures are closed. A separate audit verifies every frozen artifact/source/dependency unchanged and identifies only two extra metadata/library keys in the coordinator layout; the raw manifest failure stays preserved. A disposable expectation-only copy probe uses the independently derived6*root formula (439-byte operation;455 including guard) and passes50/0, including0/exact/one-under, source validity, rollback and balanced roots. Original375 result stays failed historically; runtime charging is not weakened.

Follow-up plans reviewed as mutually exclusive: [borrow-nominal](borrow-nominal/plan.md) owns two sema resolution modules/new tests; [recovery-roots](recovery-roots/plan.md) owns runtime graph/private scope helper/new tests; [harness-hardening](harness-hardening/plan.md) owns existing harness/new evidence, keeping old evidence immutable. Static dependency-file attribution remains assigned to the subsequent sole source producer together with aggregate ownership, copy_owned/promote and authenticated v3 completion. No extra language consumer is ready. All R01–R15 remain required.

Final combined historical replay passes136/0 (all18 required regression commands), package40/0 and review15/0. Exact reports, all attempts and integrity manifests are retained in evidence/combined-reviewed/. Every failed invocation, partial report and setup correction is retained separately. Actual dispatch IDs/status follow in dispatch.md and thread-state.json.

## Nominal constructor follow-up accepted

Worker35a8618 accepted/integrated6ffbf16 for bounded OM04 completion. The same shared `region_borrow` resolution guards rewriting/provenance while normal constructor types and region brands remain checked. Two independent reviews verify425Rust/243new/196admission/117ordinary/96API/2289audit passes,328archives/1360streams/11artifacts/243dependencies/795compilerinputs/113executables/45quota files. The API count changes only because11artifacts produce22hash assertions versus prior5/10. All23 standalone programs genuinely ran outside repositories; old inside-checkout rows remain historical emitted-Rust parity. Failed and truncated quota evidence stays explicit.

Coordinator compared all795 compiler inputs to the integrated checkout, verified frozen artifacts/dependencies, then independently replayed243/0 source/package interpreter/native debug/release/standalone assertions in fresh external scratch without rebuilding the frozen candidate. Exact evidence in evidence/borrow-reviewed/. No independent coordinator425-test rebuild is claimed. Runtime recovery and harness hardening remain active; full follow-up integration and broad source/release requirements remain open. No extra consumer frontier or duplicate task was launched.

## Recovery reviewed; harness finalization repair required

Runtime0503ed9 is accepted as a bounded repair and integratedded3768. Independent source review and4 runtime tests plus73 fault executions pass;852 exported entries,12 canonical artifacts,435 dependencies and205 compiler inputs verified. Worker515/0 recovery,957/2 matrix,135/1 historical with nested670/8, and separate133/0 reached-reserve reconciliation remain distinct. The two matrix failures are obsolete375 copy budget and static dependency filename. No broad memory acceptance.

Harnessd43eb9f is stagedb552fcf as truthful incomplete evidence.393 historical files are unchanged;332 primary plus30 input archives and808 stream pairs verified;50 lane/supplemental plus8 historical selftests pass. Actual971/11 main and1326/34/6 workspace remain raw outcomes. All14 observed sentinel rejections and toolchain identities are valid. However, production sentinel_status accepts duplicate/mixed/unrelated diagnostic-code mentions, and Journal.finish can accept timeout/signal/overflow rows. Required toolchain success and hardcoded89464a7 candidate provenance also need repair. See harness-finalization/plan.md, independently reviewed with no material gap. Retained evidence is not fabricated, but validators are not accepted.

Review attempts are also retained as reported limitations:50-test /tmp run22 quota errors, one-test retry11 quota subcase errors, private-target retry50/0, historical8/0. Final independent frozen audit passes1370 artifact/dependency entries,214 compiler,35 harness and68 toolchain entries. One sandbox startup failed on .aws quota; a shell heredoc quota failure with exit0 was not counted as success. Reviewer scratch self-cleaned; full reviewer observations exist in chat tool history, not a persistent raw audit file. Runtime independent73-case log save was interrupted by /tmp quota after65536 bytes; partial /tmp/cgx2-recovery-independent-0503ed9.json remains, not a complete archived report.

Coordinator combined source atded3768 is undergoing fresh offline workspace tests, Clippy/fmt/build in target/ordinary-finalization-combined-20261024 with jobs2,Rust1.98.1,incremental0 and dev/test debug0 to limit disk use. The invocation records exact source and flags; harness reviewed_base is stale and explicitly superseded by coordinator-invocation.json plus source hashes. Results and dispatch follow only after inspection. No S3 consumer, main merge or release. R01–R15 and all source ownership/transfer/authentication requirements remain unchanged.

## Combined follow-up review complete; one harness repair ready

Compiler candidateded3768 passes1383 workspace tests/0 failures/6 ignored, all-target/all-feature Clippy, fmt and build. Final frozen main matrix962/1 has only dependency-negative-exact-file (lib.cg required, main.cg observed); source recovery passes. Native sweep shortens to the actually reached20 reserves plus unaffected21 after the removed fallible registration. Auto32/0, package43/0, OM01–08 review19/0, nominal243/0 and admission196/0 pass. Historical raw135/1 and nested670/8 remain unchanged; all18 regression commands executed. Separate reached-reserve reconciliation133/0 passes without rewriting either original result.

Focused recovery initially passed463 assertions then raised an exception because its helper asserted a four-root formula rewrite changed text; the concurrently hardened probe already uses6*root. The original nonzero run and runner bytes remain retained. Coordinator changed only that assertion to require exactly one old4-root or corrected6-root formula; replacement, actual budget controls and all runtime expectations are unchanged. Independent runtime reviewer approved this cross-harness compatibility fix. The complete new recovery run passes515/0 against identical frozen artifacts. Original report's complete:true does not negate its recorded exception/nonzero exit.

Coordinator audit checks every completed command outcome and stream hash, all14 actual sentinel diagnostics and all recorded frozen inputs: no abnormal commands, stream mismatches or identity changes.536 compressed evidence archives are hash-verified in evidence/finalization-combined; acceptance.json separates all raw failures, exceptions and reconciliations. Candidate/compiler identity is ded3768 plus exact recorded source hashes; the old harness89464a7 label remains explicitly stale. Driver startup first hit /tmp write quota and was moved to a private writable target; no source expectation changed.

Recovery is accepted only as a bounded repair. Harness acceptance remains failed for reproduced duplicate/mixed diagnostic acceptance, abnormal-command finalization and provenance. The independently reviewed harness-finalization plan is the only dependency-ready new chat. No S3 consumers, broad ownership/transfer/v3 acceptance, main merge or release. R01–R15 remain required. The next source capability producer follows successful combined harness acceptance; aggregate ownership precedes source conversions because conversion depends on executable typed owners, partial-move cleanup, loans and authenticated calls.
