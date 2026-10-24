# Native graph guard budget repair

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/{README,REVIEW}.md, plans/release-completion/{README,contracts,worker-rules}.md, s2b-memory/plan.md and s2b-memory-review/README.md. MegaMemory overview/query/record and Serena initial instructions are required. Work only in this chat's private worktree on a riley/ branch. No delegation, new chats, main merge, publication, package installation, external API spend or security changes. Preserve root Cargo files and coordinator/shared docs/plans; new already-local dependencies may be added only in owned manifests, exclude generated lock edits and report reconciliation.
Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, private CARGO_TARGET_DIR=$PWD/target/release-LANE and TMPDIR=$PWD/target/tmp-LANE (create both); LIBRARY_PATH=/tmp/cgx2-wave22-lib when needed. Commit code and owned evidence, report exact checks/limitations/commit and stop for review. Existing R01-R15, full ordinary-memory requirements, remaining S2b/S3/S4 gates stay mandatory.

## Exclusive ownership
- crates/cgx2-emit-rust/src/memory.rs
- crates/cgx2-emit-rust/tests/graph_guard_parity.rs (new)
- crates/cgx2-emit-rust/tests/graph_guard_parity/** (new fixtures/runner/evidence)
All other paths read-only, especially memory_runtime.rs.txt, CLI memory.rs and all ordinary-owned modules. Parallel source-owned and reporting lanes must remain independent. No AST/type/runtime API changes.

## Defect and deliverable
Coordinator fixture ../evidence/nested-zero.cg succeeds as native debug but interpreter fails CGR013 BudgetExceeded at span179..251. emit_block always emits session.no_gc; graph interpreter selects try_no_gc. For graph_memory_profile modules use Session::try_no_gc for every lexical/callee guard, preserving original source span, CGR013 failure and cleanup ordering. Nongraph legacy exact-budget behavior remains compatible. Compute/pass immutable profile explicitly; never infer from runtime payload or only the current function.

## Required verification
Use real .cg interpreter/native debug/native release/standalone Rust (authenticated existing runtime sources), compare diagnostic, failure span, cleanup roots and deterministic memory counters. Cover outer0/inner0 failing entry, exact and one-under cumulative guard-table budgets, graph invocation through scalar caller/callee and legacy nongraph compatibility. Cover GuardBudgets reserve injection with supported library test hooks, labeled synthetic. No resource-failure pass from crash/signal. Python unsupported legacy memory-script cases must exit normally with the existing CGE001 memory-unsupported diagnostic and preserve prior artifact. Run owned offline all-feature tests and all-target Clippy -D warnings, fmt/diff; existing 99 graph product runner and applicable legacy memory tests must pass. Record actual counts and commands. This repairs native guards only; it does not complete ordinary memory.

## Coordinator diagnostic correction — 2026-10-24

The initial plan incorrectly requested CGE006 for these legacy memory scripts. Inspection of cgx2-emit-python/src/emit.rs and an independent real invocation confirm CGE001 is the existing explicit memory-unsupported diagnostic; CGE006 belongs to a different feature/profile path. The native worker correctly preserved that policy and recorded the discrepancy. Acceptance here requires normal exit1, the specific CGE001 unsupported diagnostic and exact artifact preservation, verified for all17 cases. This corrects the plan rather than changing source admission or silently suppressing a failure. Historical worker evidence remains unchanged. The ordinary-owned plan retains its separate CGE006 requirement.
