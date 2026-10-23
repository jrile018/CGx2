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
