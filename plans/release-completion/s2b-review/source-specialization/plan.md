# CGx2 S2b source specialization repair

Status: bounded follow-up repair; foundation acceptance remains failed.

## Exclusive ownership

crates/cgx2-sema/**; only if needed crates/cgx2-syntax/src/canonical.rs and new crates/cgx2-syntax/tests/s2bf_specialization.rs

## Deliverable

Repair the source-to-concrete-specialization boundary while preserving checked public source admission and all shared representations.
1. A named standalone module's concrete types are qualified twice. The integrated fixture crates/cgx2-api-index/tests/fixtures/s2bf-public.cg checks successfully but run/emit reject public_api.public_api.Value. The small ../evidence/round2/qualified.cg with module out.main, record Long(x:String), main()->List<Long> returning [Long(x:"é")] similarly rejects out.main.out.main.Long. Fix canonical identity handling through concrete TypeSyntax and lowered module rechecking, including dotted module names, record fields, variant payloads and nested containers. Canonicalization must be idempotent for split paths and single dotted identity segments, including already-lowered nested types; it must not grant access or change nominal identity.
2. Checked calls of a public generic using a caller's private local type are valid internal instantiations, but concrete specialization rechecking incorrectly diagnoses an exported private nominal signature. Fixtures s2bf-visibility.cg and s2bf-package in cgx2-api-index/tests/fixtures check/API successfully but run/emit reject CGT022. Keep original public declarations and their privacy checks intact. Both generated function and aggregate specializations (including Wrapper<Secret> and Choice<Secret>) must preserve body/effect/provenance checking without becoming public source exports. No blanket suppression of CGT022, no publicization of private types, no trust bypass. Public API must still omit private owner/callee/type metadata; internal specialization evidence remains complete.
Reproduce first on the integrated base. Cover same-named modules/types/binders, nested generic functions and aggregates, unused/used parameters, source/package routes, cross-package callers and legitimate private types at defining/calling modules. Negative public signatures exposing private types, unknown/private imported types, dishonest effects and stale imported hashes must continue rejecting before artifacts. No new source features or shared required fields.
Acceptance: owned Rust tests/Clippy/format/diff; actual interpreted/native debug/native release/standalone Rust/Python executions outside repository cwd for all affected fixtures; API visibility runner must remove all15 prior execution failures without weakening metadata assertions; qualified-source probe in CLI output runner must pass admission and reach CGR010 for the large fixture. Original standalone small qualified fixture must return its exact display. Re-run foundation159 checks and package binder88 checks. The semantic-resolution native constructor failures belong to the concurrent native-constructor lane; preserve/report any remaining failure rather than editing its crate.

## Rules

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/README.md and REVIEW.md, plans/release-completion/{README.md,contracts.md,worker-rules.md}, s2b-foundation/plan.md, s2b-review/README.md and this assigned plan. MegaMemory overview/query/record and Serena initial instructions required.

Use attached worktree and riley/ branch. All paths not explicitly owned are read-only, including root Cargo.toml/Cargo.lock and coordinator ledgers. Freeze Type, TypedModule, ResolvedCall, AST/imported summary layouts, constraints, source admission, canonical specialization keys and schema versions. No new dependencies or cross-owner edits. Work yourself; no delegation, new chats or follow-on task.

Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=$PWD/target/release-source-specialization, TMPDIR=$PWD/target/tmp-source-specialization; create private directories. LIBRARY_PATH=/tmp/cgx2-wave22-lib if required. All checks offline/locked. Native compiler must have sufficient address space; record capped infrastructure failures separately and do not call them passes. No performance measurements, installation, external paid APIs, contacting people or security changes.

Record actual commands, output and test counts in owned REPAIR-S2BF-ROUND2.md with reproducible runner/evidence. Preserve rejected artifacts. Do not change shared historical evidence files; write new results under an owned directory. Commit and stop for coordinator review. Keep independent failures explicit; do not weaken acceptance or claim whole foundation/release complete.
