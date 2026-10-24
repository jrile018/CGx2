# Ordinary-memory runtime root repair

## Exclusive ownership
- crates/cgx2-runtime/src/graph.rs and crates/cgx2-runtime/src/scope.rs
- New crates/cgx2-runtime/tests/ordinary_roots_repair.rs
- New crates/cgx2-sema/tests/ordinary_roots_repair.rs (source runtime tests only)
- New crates/cgx2-syntax/ordinary-roots-repair-evidence/** and S2B-ORDINARY-ROOTS-REPAIR.md
No type/provenance, public model, ownership-v3, CLI/emitter or other source changes. Preserve existing safe APIs and root obligations.

## Required result
1. OM05: align environment binding lifetime and root-table ownership across region scopes. Updating an outer managed binding from a region arm must preserve the outer lifetime and release the old registration through its actual owner. Test both graph-free and heap RHS, nested regions/guards, repeated assignment, return/error cleanup. Do not swallow NotRooted, leak roots until exit or weaken brand safety. Region-local values cannot escape via outer cells.
2. OM06: copy_graph_to_v1 must retain temporary protection of its valid input through all destination allocation/root-publication safepoints, including same-region Auto trigger_bytes1. Caller is not obliged by current safe API to pre-root the valid handle. Source bytes/topology/aliases remain valid after success and failure; return/publication roots and guards remain balanced. Test Manual/Auto, different/same region, cycle and reserve failures. Preserve copy/promotion transactional semantics without changing unfinished source promotion interfaces.
3. OM07: accurately account the root created by copy publication. Registered/released deltas must balance on normal and failed paths along with baseline/final root census and zero cleanup failures. Never normalize counts or omit releases to make evidence green.

## Verification
Run runtime/sema/memory-conformance offline all-feature tests; all-target/all-feature Clippy -D warnings for those packages, fmt/diff checks. Reproduce direct API copy-root probe using canonical library; add fault/trigger tests with exact source survival, destination rollback and root deltas. Exercise shipped copy-cycle and outer-assignment repro across interpreter/native debug/native release/standalone via existing frozen APIs. Run existing runtime region/graph/ordinary suites and unchanged ordinary-memory product runner. Retain independent static-admission/API defects as failures; do not edit other lane sources.

## Common worker contract

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/{README,REVIEW}.md and plans/release-completion/{README,contracts,worker-rules,dispatch}.md, s2b-ordinary-memory/plan.md and this review README before coding. Follow MegaMemory overview/query/record and Serena initial instructions. Work only in your attached worktree on a riley/ branch. No delegation, other chats, merges, follow-on tasks, publication, package installation, external spending/contact or security changes.

Use private CARGO_TARGET_DIR=$PWD/target/release-ordinary-LANE and TMPDIR=$PWD/target/tmp-ordinary-LANE (substitute lane), CARGO_BUILD_JOBS=2, RUSTUP_TOOLCHAIN=1.98.1, PYTHONDONTWRITEBYTECODE=1 and existing LIBRARY_PATH=/tmp/cgx2-wave22-lib when required. Create directories; offline/locked dependencies. No release performance measurements. Other lane paths, root Cargo.toml/Cargo.lock, shared design and all coordinator plans are read-only. Preserve historical evidence. Freeze final binaries/probe libraries before product runs and verify hashes afterwards; do not build Cargo during frozen verification. Use canonical top-level probe libraries, never an arbitrary stale hashed dependency. Retain every failed attempt, bounded stdout/stderr, wall/CPU/address/file limits, argv/exit/signal, sources and toolchain/compiler/runtime hashes. Existing output supervisor and freeze helper are available read-only.

Finish the bounded deliverable, run assigned checks, record specific APIs/limits/evidence in MegaMemory, commit owned files/report and stop for review. Report commit, changed paths, exact counts, failed/blocked/unrun rows and limitations. Passing this repair never accepts full ordinary memory or release. General aggregate ownership, source copy_owned/promote with failed-owner continuation, complete authenticated v3 summaries and static dependency-file attribution remain assigned to a later sole producer. Do not implement or silently waive them here.
