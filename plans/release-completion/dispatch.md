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
