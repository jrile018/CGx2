# CGx2 S2b constructor resolution repair

Status: ready for bounded repair; foundation is not accepted.

## Exclusive ownership

crates/cgx2-sema/**

## Deliverable and acceptance

Fix two confirmed checker admission/resolution defects, using existing type and specialization representations.
- Data intrinsic dispatch in generic.rs checks local functions but not local record constructors. A record named length with field value:i64 and main constructing length(value:42), then x.value, must retain previous constructor semantics. Cover the other new intrinsic names and imported/qualified constructors as relevant. Ensure checked resolved identities guide lowering; do not special-case just the fixture.
- Generic binder T shadows nominal record T: f<T>(x:T) with let broken=T(value:1) currently succeeds checking without a complete value type, then run/emit fail. Choose and document namespace behavior consistent with existing binder scope: either preserve a valid nominal constructor identity or reject at source checking with a located diagnostic. Never let failed members lookup silently omit an expression type. Cover records, variants, nested generic bodies and package summaries without changing their layout.
Repros: ../evidence/constructor-shadow.cg and binder-constructor.cg. Add positive/negative sema regression tests plus product checks (successful constructor must run/build correctly; invalid constructor must reject before emission and preserve artifact). Investigate early symbolic type expansion only within this bounded checker review: ../evidence/type-expansion.cg already rejects CGT030 at 14 levels under 512MiB / 10 CPU seconds. This is NOT an established failure; do not invent a blocker or run unbounded stress. If a missing early allocation guard is demonstrated under bounded limits, fix within owned paths and record concrete evidence; otherwise report unchanged.

## Execution rules

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/README.md and REVIEW.md, plans/release-completion/README.md, contracts.md, worker-rules.md, s2b-foundation/plan.md, s2b-review/README.md and this plan before coding. Follow MegaMemory overview/query/record and Serena initial instructions.

Work only in your attached worktree on a riley/ branch. Complete this bounded repair yourself; do not delegate, create chats or launch follow-on work. All paths outside exclusive ownership are read-only, including root Cargo.toml/Cargo.lock, shared syntax/type definitions, shared docs, foundation evidence and coordinator plans. Preserve TypedModule, Type, ResolvedCall, imported summary shapes, specialization keys and source admission contracts. Do not introduce dependencies or change public schema versions. Missing cross-owner changes must be reported precisely, not edited.

Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=$PWD/target/release-LANE and TMPDIR=$PWD/target/tmp-LANE (replace LANE with your lane; create directories). LIBRARY_PATH=/tmp/cgx2-wave22-lib if needed. No shared target directory, package installation, external paid calls, host security changes or performance measurements.

Run owned package tests with --all-features --offline --locked, owned all-target/all-feature Clippy --offline --locked -- -D warnings, owned formatting and git diff --check. Build cgx2-cli/cgx2c in your private target when product checks need them. Execute applicable source/native debug/release/Python cases outside repository-relative assumptions. Preserve existing artifact on rejected emit/build. Record exact commands, outcomes and remaining cross-lane failures in an owned REPAIR-S2BF.md plus reproducible tests/fixtures. Do not weaken checks or count known other-lane failures as passes. Coordinator runs full combined workspace and product gates after integration. Commit and stop; report commit, files, checks and limits.
