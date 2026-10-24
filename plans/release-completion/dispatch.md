# Stage 1 dispatch ledger

Date: 2026-10-23. Shared implementation/plan base: `0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.

All **12 create_thread requests succeeded** and returned client IDs. After a bounded wait, Git confirms 12 new worktrees at the common base. App list_threads has not yet exposed their permanent thread IDs; classify them **queued**, not running. Do not create duplicate chats or pass client IDs to wait_threads. When permanent IDs appear, record their mapping and use compact wait_threads snapshots for review readiness.

| Lane | Requested title | Client chat ID | Status |
| --- | --- | --- | --- |
| [core](core/plan.md) | CGx2 release core commands | `client-new-thread:765516b2-673c-4ea5-98ad-ecad6a429c87` | queued |
| [memory](memory/plan.md) | CGx2 fallible runtime and lifetime completion | `client-new-thread:fc9ab61e-7f4e-4ae9-992f-27e8b60f225e` | queued |
| [ownership](ownership/plan.md) | CGx2 ownership analysis for application functions | `client-new-thread:3a162f4a-7f03-4bed-9e63-4a886dd24011` | queued |
| [native](native/plan.md) | CGx2 native binding SDK and direct inference | `client-new-thread:3435c2c2-fcaa-46be-9c4b-888ef912a85f` | queued |
| [python](python/plan.md) | CGx2 pinned Python package runtime | `client-new-thread:3b796c78-ac7f-4c99-b45b-0135e9936299` | queued |
| [worker](worker/plan.md) | CGx2 persistent worker transport | `client-new-thread:0a7a2881-f4c7-4be3-bc21-64ad934bc189` | queued |
| [execution](execution/plan.md) | CGx2 cancellable CPU execution and profiling | `client-new-thread:4239700e-e246-40f8-ab2d-a9f547ea92d4` | queued |
| [notebook](notebook/plan.md) | CGx2 retained notebook runtime contracts | `client-new-thread:291b6edf-f434-4904-90e3-62a32f792018` | queued |
| [verification](verification/plan.md) | CGx2 useful scalar proof profile and live solver runner | `client-new-thread:c6a7bcef-7f3d-49ef-8a65-90410873fb11` | queued |
| [analysis](analysis/plan.md) | CGx2 package effect and security analysis | `client-new-thread:35d57f49-143e-4946-b6e2-1538f6c4befd` | queued |
| [evidence](evidence/plan.md) | CGx2 executable release evidence pipeline | `client-new-thread:00b15e03-2204-41d5-b2e5-b0cc9865d7a0` | queued |
| [docs](docs/plan.md) | CGx2 executable release guide and acceptance map | `client-new-thread:d5c9f198-1028-494d-8f35-7df88ae3ae1e` | queued |

Every prompt assigns the committed detailed plan, exclusive paths, frozen interfaces, private targets/TMPDIR and CARGO_BUILD_JOBS=2, offline checks, evidence, commit and stop. No automatic follow-on dispatch or merge is configured. Future stages and ecosystem adapters remain dependency-gated as specified in README.md.

## Stage 1 resolution and review — 2026-10-23 22:30 UTC

All twelve workers completed. IDs were resolved from exact worktree metadata and completion confirmed with wait_threads. The original queued table is historical. Nine bounded deliveries are reviewed; three need the repairs in [s1-review](s1-review/README.md). Full cursor state is in thread-state.json.

| Lane | Permanent chat ID | Original commit | Current status |
| --- | --- | --- | --- |
| core | `01a16989-657f-7e42-8467-7d3821f0005a` | 8fdbd78 | reviewed-bounded-delivery |
| memory | `01a16989-9e7d-7670-a1d6-d54eaa476fad` | 26f654b | reviewed-bounded-delivery |
| ownership | `01a16989-b394-78a3-9c83-f29ab5232dd8` | f6fd925 | reviewed-bounded-delivery |
| native | `01a16989-bdfd-7393-9c50-d6205bec1e5a` | 4bc27a9 | reviewed-bounded-delivery |
| python | `01a16989-cd7d-7413-8cbf-c5ffd421c23d` | 1d81231 | reviewed-bounded-delivery |
| worker | `01a16989-d9a0-7f11-8a84-71d5ecb302d7` | 055adc6 | reviewed-bounded-delivery |
| execution | `01a1698a-1ae2-7d70-9870-7575c8b52753` | b597342 | reviewed-bounded-delivery |
| notebook | `01a1698a-2507-75c3-a3eb-9d8c74ddbd43` | ff3e715 | reviewed-bounded-delivery |
| verification | `01a1698a-34d3-7f23-9b5b-256b582cedcd` | eab9acd | reviewed-bounded-delivery (repair below) |
| analysis | `01a1698a-44ec-7942-ba44-753f20ea5acd` | 59446db | reviewed-bounded-delivery (repair below) |
| evidence | `01a1698a-4e64-7e00-ab15-a0ae0052bf36` | d224bf1 | reviewed-bounded-delivery |
| docs | `01a1698a-673a-7593-b8b7-a4c53f09a8d0` | 9e96ac4 | reviewed-bounded-delivery (repair below) |

## Repair wave dispatched

Base: `2b9855c48713794d4cf819bc391813c1260f287b`. All three worktrees and permanent chats are confirmed active; original client IDs are preserved in thread-state.json.

| Repair | Permanent chat ID | Worktree | Status |
| --- | --- | --- | --- |
| verifier-size | `01a16aed-eaa7-77f0-94cc-0a40c1daa7b0` | /home/john-riley/.codex/worktrees/18ee/lang | reviewed-bounded-delivery |
| analysis-contracts | `01a16aed-f621-7703-962a-c328604fef77` | /home/john-riley/.codex/worktrees/9bbd/lang | reviewed-bounded-delivery |
| docs-current | `01a16aee-0c58-7263-9b81-049eb0f96358` | /home/john-riley/.codex/worktrees/b26d/lang | reviewed-bounded-delivery |

## S1 repairs reviewed; S2a ready

All three repairs passed review and integrated checks; see s1-review/README.md for counts and the timing-test repair. All twelve original deliveries are reviewed. The sole dependency-ready implementation task is s2a/plan.md, which owns shared AST/type/value changes and mechanical consumer compatibility. Its base and actual dispatch outcome are recorded below after creation. S2b and S3 remain gated on reviewed interfaces.

## S2a launched

Base: 18b82a381c9f70278fad9373ad3db044b8a57693. One shared language/type chat is confirmed running: 01a16b0f-7fe8-79a3-b5d2-cf89fbb66b56, worktree /home/john-riley/.codex/worktrees/88c6/lang. Initial queued client ID: client-new-thread:0984f45a-9780-4fff-9477-c903eea48958. Worktree HEAD matches the reviewed base. The app recent list omitted the agent-created chat; read-only exact-path metadata resolved its permanent ID, validated by wait_threads. Cursor is saved in thread-state.json. No duplicate chat was created.

## S2a review: three bounded repairs

S2a delivered 16028e7 and is staged as ca46bb3, repair-required. Coordinator reproduced match-flow, native package-record and human API-output defects; see s2a-review/README.md. No dependent feature stage is accepted. Three disjoint repair plans are ready; actual dispatch IDs/status follow.

## S2a repair chats running

Common repair base: `8902cbd92533b81e801bf2c5bea725f5508fe3e2`. All three worktree HEADs match it and all three chats are confirmed active via wait_threads. Saved permanent IDs, queued client IDs and cursors are in thread-state.json. Recent-chat listing omitted these agent-created chats; read-only local metadata and read_thread resolved and confirmed their titles/paths. No duplicates created.

| Lane | Permanent chat ID | Worktree | Status |
| --- | --- | --- | --- |
| match-flow | `01a16b31-6c0c-7d23-86e6-39703eb67696` | /home/john-riley/.codex/worktrees/0020/lang | running |
| native-records | `01a16b31-71b6-71c0-bca5-fac3df7e31c2` | /home/john-riley/.codex/worktrees/7f73/lang | running |
| api-text | `01a16b31-80af-7cd1-ac39-090aae87c699` | /home/john-riley/.codex/worktrees/8386/lang | running |

## S2a accepted after repair review

All three repair commits are reviewed-bounded-delivery; original S2a is accepted with its documented source/target limits. Integrated 1078 Rust tests, 166 benchmark tests, 20 docs harness tests, 39 runnable documentation cases, 10 S2a and 33 API product checks pass; details in s2a-review/README.md. S2b foundation is the single dependency-ready core task. Remaining S2b source integrations stay explicit in README.md. Dispatch outcome follows.

## S2b foundation running

Reviewed base38cce386519832ddaf50460c1e03f38aa6a3c106. Chat01a16b46-2ebe-7c40-9efe-4619197003d9, worktree /home/john-riley/.codex/worktrees/2766/lang, is confirmed active and its HEAD matches the base. Queued client ID6c21b475-93b8-4b71-9555-18e231f9dedc and polling cursor are preserved in thread-state.json. The recent-chat list again omitted the worker; read-only local metadata and read_thread verified its identity. No duplicates. One shared core writer; remaining S2b/S3 gates unchanged.

## S2b foundation reviewed: five repairs ready

Final 414c71f staged as b0900dd, repair-required. Worker 1092 tests / 159 product checks and hashes verified; coordinator workspace compilation passes. Seven concrete defects map to five disjoint repair lanes in s2b-review/README.md. Existing shared representations frozen; full acceptance rerun follows repairs. All later S2b/S3 and external gates remain open. Dispatch outcomes follow.

## Five S2b repair chats running

Common repair base: b595724e1b086c604220cc5b165c141e65a0531f. All five worktree HEADs match; read_thread and wait_threads confirm active implementation. list_threads omitted agent-created chats; read-only local metadata resolved exact worktrees and permanent IDs, then read_thread verified title/lane identity. No duplicates. Queued client IDs and polling cursors are retained in thread-state.json.

| Lane | Chat | Worktree | Status |
| --- | --- | --- | --- |
| semantic-resolution | 01a16b7d-3f0a-7412-8753-b981f8e249f7 | /home/john-riley/.codex/worktrees/0e24/lang | running |
| package-binders | 01a16b7d-3f1f-7563-ae3d-161becff803e | /home/john-riley/.codex/worktrees/b9e0/lang | running |
| emitted-targets | 01a16b7d-56dc-7a61-b57e-02c4a2f091d1 | /home/john-riley/.codex/worktrees/a9fe/lang | running |
| interpreter-output | 01a16b7d-6ef8-77e3-984d-b2d4aad4af1a | /home/john-riley/.codex/worktrees/a94c/lang | running |
| api-visibility | 01a16b7d-83a2-7e63-88c4-b64d9d6d4c16 | /home/john-riley/.codex/worktrees/19b9/lang | running |

## First S2b repair wave reviewed; two follow-up repairs ready

All five preceding repair chats are reviewed-bounded-repair; historical running table superseded. Combined 1126 tests and 159 foundation product checks pass, but three reproduced residual defects keep foundation acceptance failed. Plans: s2b-review/source-specialization/plan.md and native-constructors/plan.md. No shared interface changes or downstream feature dispatch. Actual new chat identities follow.

## Two follow-up S2b repair chats running

Base c2a039db3ea0b4e457a9f2e0c7626db77094d313 is confirmed at both worktree HEADs. Both permanent chats are active via read_thread/wait_threads. list_threads again omitted agent-created chats; read-only local metadata resolved exact worktree identities, then app reads verified titles and activity. Queued client IDs and cursors are retained in thread-state.json. No duplicate chats.

| Lane | Chat | Worktree | Status |
| --- | --- | --- | --- |
| native-constructors | 01a16b8d-e2be-7142-b1f5-3d5c6810e806 | /home/john-riley/.codex/worktrees/5140/lang | running |
| source-specialization | 01a16b8d-d23c-7a20-a84b-50ade10292b6 | /home/john-riley/.codex/worktrees/c9ae/lang | running |

## Foundation accepted; ordinary function memory ready

Both follow-up repairs are reviewed-bounded-delivery. Combined 1141 Rust tests, all seven product suites, 166 benchmark tests, 20 docs tests and 39 runnable docs pass; blocked/future/external rows remain open. See s2b-review/README.md and evidence/accepted-integration.json. The sole dependency-ready implementation is s2b-memory/plan.md; all source/runtime ownership and acceptance obligations are concrete. Dispatch outcome follows.

## Ordinary function memory running

Reviewed base c110fd5f9ae140aa985188ca1ab526f38e15a7ca. Chat 01a16ba1-fa9a-7613-b05c-ec7612ca5734 (CGx2 S2b ordinary function memory), worktree /home/john-riley/.codex/worktrees/fba5/lang, is confirmed active; HEAD matches the base. Queued client ID and polling cursor are in thread-state.json. Recent-chat listing omitted the agent-created chat; read-only exact worktree metadata plus read_thread/wait_threads verified identity and running status. No duplicates. One core/runtime owner is the full safe dependency frontier; no consumer is launched against proposed memory interfaces.

## Memory delivery reviewed incomplete; three bounded lanes ready

74cfa2b staged as6467575. 1157 coordinator workspace tests and99 graph product checks pass, but native guard budgeting and source report provenance regressions reproduced. Ordinary-function ownership/managed storage/recoverable transfers remain missing. See s2b-memory-review/README.md. Two disjoint legacy repairs and one ordinary-owned source slice are prepared; no proposed interface consumer dispatch. Actual creation IDs/status follow.

## Three memory follow-up chats running

Common reviewed repair base9fcefbbb201f1f73f387f43672bc9b4f9d23eeb7. All three worktree HEADs match; read_thread and compact wait_threads confirm active work. list_threads omitted agent-created chats; read-only local metadata resolved exact worktrees, then app reads verified identity. Queued IDs/cursors preserved in thread-state.json; no duplicate chats. Memory checkpoint remains incomplete.

| Lane | Chat | Worktree | Status |
| --- | --- | --- | --- |
| ordinary-owned | 01a16bc8-90ff-7621-9e07-56cd23e25524 | /home/john-riley/.codex/worktrees/bd1f/lang | running |
| report-layout | 01a16bc8-7d97-7cc3-9f03-be20ce7186d7 | /home/john-riley/.codex/worktrees/b705/lang | running |
| native-guards | 01a16bc8-738b-70a0-8606-4bee2ed891f0 | /home/john-riley/.codex/worktrees/58d1/lang | running |

## Both memory repairs accepted; ordinary-owned remains running

Native guard and report provenance deliveries passed independent review and combined1163Rust/882product checks, build/Clippy/fmt. Integrated code2994ebf. Saved worker/integration commits and current cursors in thread-state.json; details in s2b-memory-review/README.md and evidence/accepted-repairs.json. The ordinary-owned chat remains active. No additional safe interface consumer is ready and no duplicate/new chat was launched.

## Ordinary-owned reviewed; recoverable source allocation ready

All three preceding chats are reviewed bounded deliveries. Ordinary-owned006d267 integrated asb84a24a plus lock reconciliation d87d8c8. Combined1179Rust/2123product checks pass, with build/Clippy/fmt. See s2b-memory-review/evidence/ordinary-owned-accepted/acceptance.json. Next dependency-ready task is s2b-owned-result/plan.md; actual setup outcome follows. Full memory/release gate stays open.

## Recoverable owned allocation running

Reviewed base 4113edee943b9ac389c5037904a016d7dba7f73c. Chat 01a16c01-762e-7df1-893e-28bac3bce872 (CGx2 recoverable owned allocation), worktree /home/john-riley/.codex/worktrees/b047/lang, is active via read_thread/wait_threads; worktree HEAD matches the base. Queued client ID/cursor saved in thread-state.json. list_threads omitted the new agent-created chat; read-only exact local metadata plus app reads verified identity. No duplicate. One shared source/analyzer/runtime producer is the current dependency frontier; consumers wait for its reviewed interface.

## Recoverable allocation reviewed; two disjoint repairs ready

Worker d88b57a staged as d825343. Three independent reviews and coordinator1229Rust/2707product checks completed; full existing matrix passes. Four reproduced schema1 symbol regressions and live evidence-runner output-bound gap keep acceptance failed. See s2b-owned-result-review/README.md. Symbol repair owns sema/admission/contracts/new tests; runner repair owns two Python scripts/new harness evidence. No new language consumer dispatch. Actual chat setup outcomes follow.

## Two owned Result repair chats running

Common repair base3e5be4d4907debad1ae5fa8bdc836208255ddb4d matches both worktree HEADs. read_thread and compact wait_threads confirm both active. list_threads omitted these agent-created chats; read-only local metadata resolved permanent IDs and app reads verified exact titles/paths. Queued client IDs and initial cursors are saved in thread-state.json. No duplicates; no consumer fan-out.

| Lane | Chat | Worktree | Status |
| --- | --- | --- | --- |
| symbol-compatibility | 01a16c7e-fe8a-7092-b805-8f546006a996 | /home/john-riley/.codex/worktrees/aa9e/lang | running |
| runner-output | 01a16c7f-3d83-7110-9385-b75ff89b6ef7 | /home/john-riley/.codex/worktrees/6eb0/lang | running |

## Runner repair reviewed; symbol repair remains running

cc69d8d accepted/integrated3cdb63c: independent review plus coordinator10harness/584product checks pass. Cursor/status and exact evidence saved. Symbol repair remains active with195 new source/target checks reported; its original shadow-builtin negative encodes the old-name regression and will need transparent coordinator expectation reconciliation after delivered code review. No additional safe consumer dispatch or duplicate chats.

## Owned Result repairs accepted; default managed source ready

Both repair chats reviewed-bounded-delivery. Combined code938c911 passes1237Rust/2928current product checks with explicit separate historical-negative reconciliation; see s2b-owned-result-review/evidence/accepted-repairs/acceptance.json. Next single shared producer plan s2b-default-managed/plan.md covers allfive foundation heap shapes, roots, reference aliases/cycles, recovery and resource effects. No unreviewed consumer fan-out. Actual setup outcomes follow.

## Default ordinary managed source running

Reviewed basec9d60229ecf3ed2bf76b29525fec0bdda41d0906. Chat01a16ca5-fff7-7a70-b960-e86e6714b71e (CGx2 default ordinary managed heap), worktree/home/john-riley/.codex/worktrees/685c/lang, is confirmed active by read_thread/wait_threads; HEAD matches base. Queued client6a37512e-ee9e-498e-ae81-995c71663b27 and cursor are saved. list_threads omitted agent-created chat; exact read-only local metadata plus app reads resolved identity without duplicate. Sole shared producer; consumers wait for reviewed implementation. Full memory/release gates remain open.

## Managed verification temporary quota relieved (2026-10-24 07:50 UTC heartbeat)

The sole managed worker remains running without a delivered commit. Both worker and coordinator sandbox startup hit /tmp user quota. Coordinator confirmed ownership and absence of process references, then removed only the inactive /tmp/cgx2-wave20-review-memory-target/debug/incremental rebuildable Rust cache (172,246,910 bytes). Normal sandbox git status now succeeds. No evidence or host settings changed. Worker notified to continue private-directory verification. Saved current cursor; no duplicate or consumer dispatch.

## Managed native boundary repair in progress (2026-10-24 08:50 UTC)

The existing sole producer reports the 65,536-element fixture passes interpreter checking/execution but native compilation exhausted the runner 4 GiB compiler bound. Worker is preserving the failure and compacting large emitted model literals with authenticated-model round-trip tests. This is worker-reported in-progress evidence, not coordinator-reviewed acceptance. Same chat continues within its emitter ownership; no duplicate repair chat or consumers launched. R03/R04 remain open.

## Managed delivery reviewed; three bounded repairs prepared

Delivery bc4ba95e710eaed3816291fdc67de1478e0456f5 is stopped, clean and reviewed. Staged as7643899037c6fd9522534c7b9125b054aab6088c for a common repair base; acceptance remains failed. Three reviews and coordinator probes reproduced source/AST authentication mismatch, schema1 user AllocationError variant regression, and intrinsic-vs-ordinary Result equality failure. Native budget/fault parity and exact rejection/artifact evidence also need repair. See s2b-default-managed-review/README.md and its three disjoint plans. Full release ledger preserved; no new language consumers. Actual repair creation outcomes follow after common-base commit.

## Three managed repair chats running

Common reviewed repair base3c1cfe2aeba2f7135e4069709bf633c082d2aab0 matches all three worktree HEADs. Compact wait_threads confirms each active. list_threads omitted agent-created chats; exact read-only local metadata plus app read_thread verified permanent identities, titles and paths without duplicates. Queued client IDs and cursors are preserved in thread-state.json. Independent baseline1268Rust/328managed product and required current regressions pass, but the reviewed correctness/evidence defects keep acceptance failed.

| Lane | Chat | Worktree | Status |
| --- | --- | --- | --- |
| core-auth | 01a16d5f-04e4-72b0-818f-d3cd2a19621f | /home/john-riley/.codex/worktrees/06b4/lang | running |
| runtime-equality | 01a16d5f-04f6-7142-97ef-27e4e39ccf5c | /home/john-riley/.codex/worktrees/d1b5/lang | running |
| evidence-parity | 01a16d5f-1615-7fb3-a8ca-f8467c61488c | /home/john-riley/.codex/worktrees/00f8/lang | running |

## Equality and evidence repairs reviewed; core still running

Runtime53d37c1 accepted/integratedc287985 after independent review plus coordinator4runtime/2typed-source tests and87target-parity passes. Evidence23b050f accepted/integrated1b922e2 after independent review and14selftests;670/8 matrix accurately retains the known core-entry span failures. Core source-auth repair remains active, finishing its report/commit. Its obsolete merged-AST layout test caller will be reconciled by coordinator with original assertions preserved. Full combined checks and managed acceptance remain pending; see s2b-default-managed-review/repairs-reviewed.json. No new consumers or duplicate repairs.

## Combined managed repair wave reviewed; ordinary-memory producer ready

Core a095a13 accepted/integrated91d1bc3, caller migration b0f1ccc independently reviewed. Combined1290Rust tests/quality/current regressions pass. Harness setup failure retained and fixed0949d5c; full frozen-artifact678repair/369managed/116core/87equality checks and16selftests pass. All8 earlier core-span failures closed. Evidence: s2b-default-managed-review/evidence/combined-accepted/acceptance.json.

Next sole dependency-ready producer: s2b-ordinary-memory/plan.md and exact18-command regressions.json. Generative region/default normalization and bootstrap-main contract holes resolved in independent review. No unreviewed interface consumers or release/main merge. Creation outcomes follow after reviewed-base commit.

## Ordinary aggregate ownership, regions and entry producer running

Reviewed common base0d321d3d1b0341121b8363a5a87eeaf674c20eec. Chat01a16da0-0923-75c3-9e49-0d0383043782 (CGx2 ordinary ownership regions and…) in /home/john-riley/.codex/worktrees/4836/lang is active via read_thread and compact wait_threads; worktree HEAD matches base. Queued client-new-thread:4fe31f5c-6a38-4f88-a1ab-6269ceb6e405 resolved through exact read-only local metadata after list_threads omitted it. Saved cursor in thread-state.json; no duplicate chat. Sole producer owns the connected source/runtime interface frontier; later consumers remain gated. Full ordinary-memory and release acceptance remain open.

## Ordinary-memory partial delivery reviewed; repairs required

a3b98d7 staged32032fd after three independent reviews and coordinator reproductions.1327 workspace tests and quality gates pass, but8 source/runtime/API defects block acceptance and broad ownership/promotion/v3/static dependency diagnostics remain incomplete. See s2b-ordinary-memory-review/README.md for exact findings and four disjoint repair plans. No S3 fanout or main merge. Actual setup outcomes follow after common-base commit.

## Four ordinary-memory repair chats running

Common reviewed repair base7bb7a480119826693db67ed503caaf7f23f6abb2 matches allfour worktree HEADs. App read_thread and compact wait_threads confirm each active. list_threads omitted these agent-created chats; exact read-only metadata and app reads resolved IDs without duplication. Queued IDs and cursors saved in thread-state.json. Independent baseline1327Rust/8selftests/136product/22standalone passes; initial standalone quota failure and partial-record retention gap remain preserved. Eight defects and full source ownership requirements keep acceptance failed.

| Lane | Chat | Worktree | Status |
| --- | --- | --- | --- |
| core-admission | 01a16e6c-b9bf-76c3-8096-5aafc5441506 | /home/john-riley/.codex/worktrees/a9af/lang | running |
| runtime-roots | 01a16e6c-b9bf-76c3-8096-5a9f6fbac892 | /home/john-riley/.codex/worktrees/c0c1/lang | running |
| api-visibility | 01a16e6c-cd2a-74e0-854c-ba26c9418640 | /home/john-riley/.codex/worktrees/2f3a/lang | running |
| evidence-parity | 01a16e6c-e179-79f0-bdff-73791dd80661 | /home/john-riley/.codex/worktrees/e7e8/lang | running |

## Ordinary public API repair accepted; admission repair reviewing

OM08 delivery21452c2 accepted/integrated64a71e7 after independent source/evidence review plus coordinator175Rust/84product passes and quality checks. Frozen/source hashes verified; worker118visibility/207specialization replays verified. Core9330300 has delivered and is under review; runtime roots and evidence remain active. No combined acceptance or new consumer frontier. See s2b-ordinary-memory-review/evidence/api-reviewed/.

## Four ordinary-memory repairs reviewed; bounded follow-ups prepared

API21452c2 and runtime f255dfe1 accepted for their assigned OM08/OM05–07 repairs. Core9330300 and harness80bd037 reviewed incomplete; staging commits and final stopped-chat cursors are saved in thread-state.json. Combined1360Rust and quality gates pass; first fake-solver failure and all setup attempts retained. Source nominal constructor, reserve10/11 recovery, and harness completeness are actionable independent repairs. Existing source/type/runtime public interfaces remain frozen. Three plans are reviewed with exclusive paths under s2b-ordinary-memory-review/{borrow-nominal,recovery-roots,harness-hardening}. Full release ledger remains required; no S3 consumers/main merge/publication. Final historical136/0, all18 regressions, package40/0 and review15/0 pass. Complete retained attempts and hash audit: s2b-ordinary-memory-review/evidence/combined-reviewed/. Actual creation outcomes follow.

## Three ordinary-memory follow-up repair chats running

Reviewed common base89464a76c9bd84a3b090ec58a14ca6be0043c6ea matches all three worktree HEADs. App read_thread and compact wait_threads confirm active work. list_threads omitted agent-created chats; exact read-only local metadata plus app reads resolved permanent IDs without duplicate creation. Queued client IDs and cursors are retained in thread-state.json. Full release requirements remain open; no S3 fanout, main merge or publication.

| Lane | Chat | Worktree | Status |
| --- | --- | --- | --- |
| borrow-nominal | 01a16ea8-819b-7261-a64a-9bf5a9bb1b8f | /home/john-riley/.codex/worktrees/98b9/lang | running |
| recovery-roots | 01a16ea8-81b2-7323-a1ed-602c72630fe9 | /home/john-riley/.codex/worktrees/2b95/lang | running |
| harness-hardening | 01a16ea8-bbf7-7381-8599-5953d15d1757 | /home/john-riley/.codex/worktrees/97d1/lang | running |

## Nominal constructor repair reviewed; recovery and harness active

35a8618 accepted/integrated6ffbf16 after two independent reviews and coordinator243/0 product replay with795 compiler inputs and all frozen artifact/dependency hashes matched. Worker425Rust and23 actual external standalone programs independently verified. Full attempt history retained; narrow OM04 and external-evidence gap closed. Runtime recovery and harness chats still running; saved cursors in thread-state.json. No new consumer or duplicate repair dispatch.

## Recovery and harness follow-ups reviewed

Both chats delivered and stopped. Recovery0503ed9 integratedded3768 is reviewed as a bounded repair; combined checks pending. Harnessd43eb9f stagedb552fcf is reviewed-incomplete; exact rejection, abnormal finalization and source identity require the independently reviewed harness-finalization successor. No new chat dispatched yet. Existing IDs/cursors updated in thread-state.json; no duplicates.

## Combined bounded repairs verified; finalization successor ready

Combined1383Rust/0failed/6ignored and quality gates pass. Main962/1 retains static dependency filename; Auto32,package43,review19,nominal243,admission196,recovery515 and separate legacy reconciliation133 pass. Historical135/1+nested670/8 and original recovery-runner exception stay retained.536 verified archives in s2b-ordinary-memory-review/evidence/finalization-combined. Only harness-finalization is dependency-ready; its reviewed plan freezes candidate reuse and exclusive paths. Dispatch IDs and actual setup status follow after common-base commit. No broad memory/S3/release claim.

## Evidence finalization successor running

Common reviewed base9f0804feac1a3c549ed879a8738e0152b370ed4a matches the new worktree HEAD. Chat01a16ef0-b7c6-7e22-a98c-2cbe9ddf98b5 (client-new-thread:57eab7df-68c4-4f30-8967-60480c4591de) is active in /home/john-riley/.codex/worktrees/9929/lang, confirmed by app read_thread and compact wait. list_threads omitted it; exact read-only metadata resolved permanent identity without duplication. Cursor saved in thread-state.json. One ready lane only: exact diagnostics, abnormal completion and provenance under harness-finalization/plan.md. No new language consumers or main/release action.

## Harness finalization reviewed and integrated

Harness4c4f07e accepted/integrated1e283aa after independent77selftests/11production probes/46evidence checks and coordinator fresh Auto34/0, package45/0, review21/0, freeze6/0. All inventories, command streams and source/artifact identities pass. Exact ded3768 compiler reused; prior1383Rust/0failed/6ignored and quality, recovery515/0, nominal243/0, admission196/0 evidence remains attributable to that build. Fresh worker main964/1 and historical135/1+nested670/8 remain red; separate133/0 reserve reconciliation remains separate. All766prior evidence files unchanged and2068new worker archive files verified. Reviewer failed attempts preserved. See evidence/finalization-accepted/acceptance.json and its hash-verified archives. Full ordinary-memory, source conversions, authenticated transfer summaries, S3/S4 and R01-R15 stay required; no main merge/release.

The next sole source frontier is aggregate ownership A; conversions B and all consumers wait for accepted source/storage/summary interfaces. The concrete reviewed plan and dispatch outcome follow before any new chat is counted running.

## General aggregate ownership producer plan reviewed

The sole dependency-ready next checkpoint is [S2b-A general ownership](s2b-general-ownership/plan.md), reviewed independently in its plan-review.md. Contextual fresh owned construction, partial field/index moves, projected loans/reborrows, all-body SCC summaries, precise SourceId/UTF-8 locations and Session-accounted authoritative aggregate storage are frozen together. Existing v1/v2/v3 public APIs remain unchanged via additive v4/aggregate_v1. Source memory.copy_owned/promote form dependent B after accepted A; no consumer or B chat is ready now. Required source/interpreter/native debug/release/actual external standalone, fault/cleanup, full workspace and immutable regressions are explicit. All R01–R15 remain required. Actual creation/setup identity follows the committed common base.
