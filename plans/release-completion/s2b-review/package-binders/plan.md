# CGx2 S2b imported generic binder repair

Status: ready for bounded repair; foundation is not accepted.

## Exclusive ownership

crates/cgx2-package/**

## Deliverable and acceptance

Preserve generic binder scope while constructing exported record fields and variant payload signatures. lib.rs currently qualifies T in Box<T> as local nominal app.dep.T when a same-named public record exists. Match syntax canonicalization's binder-aware exclusion without changing ImportedSignatures shapes, trust/source hashes, effects or specialization keys.
Use ../evidence/binder-package: public record T(value:i64), Box<T>(value:T), imported app.dep.Box<i64>(value:42) must check and run 42. Cover variants, nested containers, qualified non-shadowed nominal types, distinct modules with colliding names, constrained generic functions and stale source summaries. Add package tests and actual check/run/native debug/release/Python parity for representative imported generic record and variant cases. Do not edit sema or emitters; report independent known failures.

## Execution rules

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/README.md and REVIEW.md, plans/release-completion/README.md, contracts.md, worker-rules.md, s2b-foundation/plan.md, s2b-review/README.md and this plan before coding. Follow MegaMemory overview/query/record and Serena initial instructions.

Work only in your attached worktree on a riley/ branch. Complete this bounded repair yourself; do not delegate, create chats or launch follow-on work. All paths outside exclusive ownership are read-only, including root Cargo.toml/Cargo.lock, shared syntax/type definitions, shared docs, foundation evidence and coordinator plans. Preserve TypedModule, Type, ResolvedCall, imported summary shapes, specialization keys and source admission contracts. Do not introduce dependencies or change public schema versions. Missing cross-owner changes must be reported precisely, not edited.

Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=$PWD/target/release-LANE and TMPDIR=$PWD/target/tmp-LANE (replace LANE with your lane; create directories). LIBRARY_PATH=/tmp/cgx2-wave22-lib if needed. No shared target directory, package installation, external paid calls, host security changes or performance measurements.

Run owned package tests with --all-features --offline --locked, owned all-target/all-feature Clippy --offline --locked -- -D warnings, owned formatting and git diff --check. Build cgx2-cli/cgx2c in your private target when product checks need them. Execute applicable source/native debug/release/Python cases outside repository-relative assumptions. Preserve existing artifact on rejected emit/build. Record exact commands, outcomes and remaining cross-lane failures in an owned REPAIR-S2BF.md plus reproducible tests/fixtures. Do not weaken checks or count known other-lane failures as passes. Coordinator runs full combined workspace and product gates after integration. Commit and stop; report commit, files, checks and limits.
