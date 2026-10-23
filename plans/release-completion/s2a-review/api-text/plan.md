# S2a repair: complete and honest human API output

## Exclusive ownership
- crates/cgx2-api-index/**
- crates/cgx2-api-index/S2A-TEXT-REPAIR.md

All CLI sources and other crates are read-only. Keep JSON schema/version and public model shapes stable; consume their existing metadata.

## Confirmed defects and required result
1. render_text hardcodes [runtime-checked] for requires/ensures and omits semantics_status. build_syntax_index correctly stores status=unchecked. ../evidence/api.cg is deliberately unsupported generic syntax: human api --syntax-only labels its contracts runtime-checked, while JSON correctly labels them unchecked. Render actual stored contract status and clearly identify syntax-only versus checked output.
2. Human variant payloads print ApiType.identity without recursive arguments. Option<Result<i64,Bool>> becomes cgx2.core.Option. Human output also loses generic type parameters and named constraints on functions/records/variants. Render complete nested applied types and declarations deterministically using the model, including generic constraints, relevant loop-contract metadata/status and canonical qualified nominal identity.
3. Review all human-rendered statuses/signatures for consistency with JSON, including checked versus syntax-only APIs and public/private filtering. Do not merely change all statuses to unchecked. Preserve exact checked metadata when available; do not claim execution or proof from syntax indexing.

## Acceptance
- Exact human-output regressions for checked and syntax-only functions with requires/ensures; generic records/variants/functions with Eq+Show; nested Option<Result<...>>/qualified aggregate payloads; loop annotations and deterministic repeated output. Confirm JSON unchanged and truthful.
- Run cgx2-api-index tests and all-target/all-feature Clippy -D warnings; compile/run existing cgx2 CLI api commands against owned fixtures (CLI sources read-only), checking both text and JSON. No new dependency required. Owned formatting and git diff --check.

## Shared rules

Read ../README.md, ../../README.md, ../../contracts.md, ../../worker-rules.md, AGENTS.md and the design/validation requirements. This is a bounded repair of staged S2a, not approval of its interfaces for S2b/S3. Work only in your attached worktree on a riley/ branch. Preserve all public AST/type/value shapes, schemas, admission contracts and unrelated semantics. Root Cargo.toml/Cargo.lock, other lanes, shared docs/plans and coordinator evidence are read-only. No installs, paid APIs, host security changes, new chats, delegation or release actions.

Create private CARGO_TARGET_DIR=$PWD/target/release-s2a-REPAIR and TMPDIR=$PWD/target/tmp-s2a-REPAIR, substituting your lane name; set CARGO_BUILD_JOBS=2 and RUSTUP_TOOLCHAIN=1.98.1. Use LIBRARY_PATH=/tmp/cgx2-wave22-lib if needed. Do not share targets or measure release performance. Use existing offline dependencies. Record precise commands/results and limitations in your owned report, commit and stop for review. Source fixes must preserve existing positive support and reject invalid code before backend publication.
