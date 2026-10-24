# S2a review and bounded repair frontier

2026-10-23 23:50 UTC heartbeat. Delivery 16028e7d4c78f3974c9e4a2f24878debeec56612 is staged on coordinator as ca46bb35bc301b5b3282aee3a1af550b47a6773c, **repair-required**, not accepted for S2b/S3. Ownership inspection found all 60 committed paths inside assigned ownership; .serena remains untracked in the worker and was not integrated. Root Cargo files/dependency graph unchanged.

## Evidence checked
Worker logs contain 1059 passed/0failed/6ignored across 232 suites; Clippy/fmt/diff pass and 10 product entry checks are recorded. Coordinator all-target/all-feature workspace cargo check --offline --locked passes on the staged candidate. Static reviewers checked core, runtime/native and downstream/API paths independently. These checks do not override the following coordinator-reproduced failures.

## Confirmed blockers
| Lane | Finding | Required outcome |
| --- | --- | --- |
| match-flow | Non-final assignment match wrongly requires function result; inferred early Boolean return inside match is omitted, so check passes but run/native fail. All-return private match helpers also miss inference. | Correct tail/return context and complete control-flow result inference; source diagnostic before code generation for inconsistent types. |
| native-records | Package record field types are emitted from uncanonicalized syntax. Holder(flag: Flag) interprets 42 but native fails E0425. | Canonical names/types for all package aggregate field declarations with nested/imported parity. |
| api-text | Syntax-only human output says runtime-checked for unchecked contracts; nested type arguments/generic constraints disappear. | Human output faithfully renders complete model signatures and stored semantic/contract statuses. |

Reproduction inputs and captured outputs are in evidence/. report.json preserves the initial package setup failure before missing declared modules were supplied; package-report.json is the valid final reproduction. inferred-report.json uses the required else clause and demonstrates check 0 / run 1 / build 1. api-report.json includes human and JSON outputs. Binaries came from the delivered worker private debug target; this is functional repro, not performance or solver evidence.

Three disjoint repair plans are ready: [match-flow](match-flow/plan.md), [native-records](native-records/plan.md), [api-text](api-text/plan.md). Public AST/type/value shapes are frozen for these fixes. Repair chats may use the staged baseline to fix these specific defects; dependent S2b/S3 feature work remains blocked until combined repair review and integration gates. No R-row closed. Existing external prerequisites remain unchanged.

All three repair chats are confirmed running from8902cbd; see ../thread-state.json and ../dispatch.md for IDs and cursors.

## Repair closure — 2026-10-24 00:10 UTC heartbeat

All three repairs are reviewed and integrated; S2a is accepted as a bounded language/type delivery. Original commits: match-flow 03f7607 → 38b5ff0, native-records d4e61ba → 35839ad, api-text 7015726 → d2853ba. Ownership and evidence were independently inspected; no new dependency/root lock reconciliation was required.

- Combined workspace all-feature offline locked tests: **1078 passed, 0 failed, 6 ignored**, 238 suites.
- Workspace all-target/all-feature Clippy with warnings denied, formatting/diff checks and default-feature binaries: pass.
- Benchmark suite: **166 passed** after refreshing only the current cgx2c source-tree pin for the still-unrun public syntax pilot. Initial run correctly failed on old tree 98d1c0a89dfaa6f752519917adf6ef2e04b32532 versus current 15078906ea1ae6e6e8fe6c111e780e2f76962216; compiler/Cargo executable pins and Cargo.lock hash were unchanged. Mismatch evidence is preserved.
- Documentation harness: **20 passed**; integrated executable docs: **39 passed, 0 failed, 6 blocked, 7 future**, acceptance_complete=false.
- Product S2a runner: **10/10**; API text/JSON/source/native/Rust/Python runner: **33/33**.
- Coordinator original repros: **6/6**. Non-final match returns 42; inconsistent inferred return rejects CGT003 before backend; local aggregate package interprets and builds/runs 42; syntax-only API reports unchecked contracts and complete nested/generic types.
- Public syntax readiness: toolchain identity and all 6 reference checks pass. Model traces 0, independent human review unrun, scoring isolation unsupported (cpu/memory/pids not delegated). No release/model claim.

Integrated code commit: d2853ba60f4cb76a903f26288ceb31af47ecc682. Durable current reports are evidence/accepted-*.json; raw logs are /home/john-riley/projects/lang/target/s2a-repairs-*. No leak, live-solver, GPU, quiet-host performance, or genuine paired-study gate is newly closed.

Next dependency-ready task is [S2b foundation](../s2b-foundation/plan.md): executable generics/constraints/data/indexing and portable value semantics, with an explicit compatibility contract. Remaining ordinary memory, typed foreign/capability/package integration, owned map/zip and installed command completion stay named S2b requirements. One shared core writer is currently safe; downstream S3 work remains gated.
