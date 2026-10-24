# S2b memory delivery review — 2026-10-24

Status: graph/payload supporting delivery and both legacy repairs accepted; ordinary-owned source slice running. Ordinary-function checkpoint remains incomplete.
Worker 74cfa2bd371f87fe22f79b0c38bfc4aa8785c862 is staged as 6467575ba6627eeafe6bffd5f8fbe6e87720fb70 on the coordinator review branch. No main merge or release. All changed paths were authorized; no dependency or root-lock reconciliation.

## Evidence and findings
Worker delivered checked byte writes, rooted chunked graph storage and explicit script/native operations. Three read-only reviewers checked core source, runtime and evidence independently. Committed evidence hashes and counts match. Coordinator reran the exact source tree: 254 workspace suites, 1157 passed/0 failed/6 ignored; default build, all-target/all-feature Clippy -D warnings, fmt/diff and graph product99 pass. The initial restricted test attempt failed transport subprocess permissions; authorized unrestricted repeat passed. Exact commands/log hashes are in evidence/review.json. Supporting tests do not establish ordinary memory capabilities.

1. Native guard parity: evidence/nested-zero.cg fails interpreter CGR013 BudgetExceeded at179..251 but native debug exits0. emit-rust/src/memory.rs always emits session.no_gc while graph interpreter uses try_no_gc. Repair native fallible guard entry and exact budget/fault parity.
2. Report provenance: evidence/forged-layout.cg creates two raw objects with one source edge, writes valid graph-like headers, then fails expectation. operation_failure reports two rooted handles but zero edges. Raw mutable bytes cannot authenticate layout. Repair CLI and emitted helper snapshots to retain trusted source classification; expected edge count1.
3. Main planned capabilities absent: ordinary owner/loan CFG lowering/summaries, ordinary managed values/roots, recoverable transfer Result paths and ordinary cleanup/package/no_gc. S2B-MEMORY.md accurately marks them missing. No checkpoint completion credit.

## Parallel frontier and ownership
Three bounded chats can share this reviewed repair base without consuming proposed compiler interfaces:
- native-guards/plan.md owns only legacy Rust memory generator and its new tests.
- report-layout/plan.md owns only CLI legacy memory interpreter and emitted runtime template plus new tests.
- ordinary-owned/plan.md owns the new ordinary source producer and interpreter/native slice, excluding all four legacy memory modules and both repair test trees. It uses already-reviewed ownership v1/OwnedBytes APIs and can express the subset with existing AST. It is the sole writer for new shared source representations. Consumers do not launch against those representations until reviewed.

All three must be reviewed and combined before a new common acceptance base. The full s2b-memory/plan.md remains the requirement ledger. After first ordinary-owned slice, generic/package/recursive CFG ownership and recoverable Result provenance, default managed source values, regions/transfers/session no_gc and cleanup closure remain explicit required work. Typed foreign/execution/commands, S3, S4 and every R01-R15 row stay mandatory. Existing external host/study prerequisites unchanged.

## Combined repair acceptance — 2026-10-24 03:10 UTC

Report-layout d0a6055 and native-guards fe9714e passed independent reviews and were integrated as0a13447 and2994ebf. Combined256 workspace suites/1163passed/0failed/6ignored, default build, all-feature/all-target Clippy -D warnings and fmt/diff pass. Actual source/target runners pass518guard +265report +99legacy checks (882total). Nested guard entry now matches interpreter/native failure; forged raw payload reports the real source edge. Evidence and exact log hashes: evidence/accepted-repairs.json. Legacy Python normal exit1/CGE001/artifact preservation is verified; the plan incorrectly named CGE006 and is transparently corrected with historical evidence retained.

The only active implementation is ordinary-owned01a16bc8-90ff-7621-9e07-56cd23e25524; last snapshot still in progress. No further consumer is dependency-ready while its new representation is unreviewed. Full ordinary managed/owned source requirements and every later release gate remain open. No new chat is created merely to replace a completed repair.
