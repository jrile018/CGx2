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
