# CGx2 S2b native constructor dispatch repair

Status: bounded follow-up repair; foundation acceptance remains failed.

## Exclusive ownership

crates/cgx2-emit-rust/**

## Deliverable

Finish target support for the reviewed semantic constructor fix. cgx2-sema now correctly resolves a record named length/show/get/etc., but Rust finite data expression dispatch still chooses an intrinsic from callee spelling unless typed.functions has that name. Original constructor-shadow.cg checks and interprets42; emitted Rust invokes cgx2_length(i64) and rustc fails E0277/E0308.
Use existing checked type/nominal/call evidence to give actual record constructors their correct initialization path; preserve ordinary functions returning records and genuine intrinsic calls. Handle all10 names length/show/parse_i64/array/list/get/append/concat/updated/slice, generic records and named fields, defining-module-qualified and imported cases, and nested use in generic bodies. Preserve evaluation order/single evaluation, inferred concrete type, checked bounds and diagnostics. Do not rewrite nominal identities to avoid collisions, reserve formerly valid names, change sema/syntax/types or broaden target admission.
Acceptance: add actual native compilation/execution regressions under owned emitter crate, not snapshots alone; run owned tests/all-feature Clippy/format/diff. Re-run crates/cgx2-sema/tests/semantic_resolution/run.py against your private built CLI/compiler: all33 previous native failures (original plus10 standalone cases at debug/release/emitted Rust) must become passes, with all prior positives and artifact-preservation negatives intact. Re-run foundation159 and emitter104 product checks. Include standalone rustc --edition=2021 plus -O execution outside repository assumptions. Source specialization fixtures are repaired by the concurrent source-specialization lane; report independent failures without editing other paths.

## Rules

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/README.md and REVIEW.md, plans/release-completion/{README.md,contracts.md,worker-rules.md}, s2b-foundation/plan.md, s2b-review/README.md and this assigned plan. MegaMemory overview/query/record and Serena initial instructions required.

Use attached worktree and riley/ branch. All paths not explicitly owned are read-only, including root Cargo.toml/Cargo.lock and coordinator ledgers. Freeze Type, TypedModule, ResolvedCall, AST/imported summary layouts, constraints, source admission, canonical specialization keys and schema versions. No new dependencies or cross-owner edits. Work yourself; no delegation, new chats or follow-on task.

Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=$PWD/target/release-native-constructors, TMPDIR=$PWD/target/tmp-native-constructors; create private directories. LIBRARY_PATH=/tmp/cgx2-wave22-lib if required. All checks offline/locked. Native compiler must have sufficient address space; record capped infrastructure failures separately and do not call them passes. No performance measurements, installation, external paid APIs, contacting people or security changes.

Record actual commands, output and test counts in owned REPAIR-S2BF-ROUND2.md with reproducible runner/evidence. Preserve rejected artifacts. Do not change shared historical evidence files; write new results under an owned directory. Commit and stop for coordinator review. Keep independent failures explicit; do not weaken acceptance or claim whole foundation/release complete.
