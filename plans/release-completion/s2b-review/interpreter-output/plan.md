# CGx2 S2b package output bound repair

Status: ready for bounded repair; foundation is not accepted.

## Exclusive ownership

crates/cgx2-cli/**

## Deliverable and acceptance

Unify finite-value finalization and output-size enforcement for standalone and package interpretation. run_module_inner checks the final value and 1 MiB output; run_package_module returns RunResult without those guards. ../evidence/output-package returns a list of 2000 records with a 600-character name: cgx2 run exits 0 and writes 1,242,001 bytes; emitted Python exits 1 CGR010 output limit exceeded.
Enforce the same existing positive bounds on both routes before oversized output is published. Use bounded rendering or a sound preflight to avoid allocating unbounded display strings (nominal names, escaping and nested display overhead count); do not truncate a successful value or change value semantics. Preserve Unicode scalar semantics but count output UTF-8 bytes consistently. Cover below/at/above cap, long nominal names, Unicode/escaped strings, nested records/variants/collections, both module and package entrypoints, and diagnostics. Verify native/Python parity for the output repro via existing backends without editing them. Keep source location behavior consistent with the established standalone contract, report any independent target limits. Add reproducible CLI tests and capped product evidence.

## Execution rules

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/README.md and REVIEW.md, plans/release-completion/README.md, contracts.md, worker-rules.md, s2b-foundation/plan.md, s2b-review/README.md and this plan before coding. Follow MegaMemory overview/query/record and Serena initial instructions.

Work only in your attached worktree on a riley/ branch. Complete this bounded repair yourself; do not delegate, create chats or launch follow-on work. All paths outside exclusive ownership are read-only, including root Cargo.toml/Cargo.lock, shared syntax/type definitions, shared docs, foundation evidence and coordinator plans. Preserve TypedModule, Type, ResolvedCall, imported summary shapes, specialization keys and source admission contracts. Do not introduce dependencies or change public schema versions. Missing cross-owner changes must be reported precisely, not edited.

Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=$PWD/target/release-LANE and TMPDIR=$PWD/target/tmp-LANE (replace LANE with your lane; create directories). LIBRARY_PATH=/tmp/cgx2-wave22-lib if needed. No shared target directory, package installation, external paid calls, host security changes or performance measurements.

Run owned package tests with --all-features --offline --locked, owned all-target/all-feature Clippy --offline --locked -- -D warnings, owned formatting and git diff --check. Build cgx2-cli/cgx2c in your private target when product checks need them. Execute applicable source/native debug/release/Python cases outside repository-relative assumptions. Preserve existing artifact on rejected emit/build. Record exact commands, outcomes and remaining cross-lane failures in an owned REPAIR-S2BF.md plus reproducible tests/fixtures. Do not weaken checks or count known other-lane failures as passes. Coordinator runs full combined workspace and product gates after integration. Commit and stop; report commit, files, checks and limits.
