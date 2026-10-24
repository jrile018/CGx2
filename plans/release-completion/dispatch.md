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
