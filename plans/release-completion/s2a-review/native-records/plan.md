# S2a repair: canonical package record lowering

## Exclusive ownership
- crates/cgx2-emit-rust/**
- crates/cgx2c/tests/s2a_package_record_parity.rs (new file only)
- crates/cgx2-emit-rust/S2A-RECORD-REPAIR.md

Do not edit sema, syntax, package, CLI or existing cgx2c tests. The match-flow lane owns cgx2c/tests/s2a_match_control_flow.rs. New test fixtures go under cgx2-emit-rust/tests.

## Confirmed defect and required result
emit_package_rust uses canonical modules for variants/functions but emits record declarations from source.syntax().records, qualifying only record.name. Local field types remain unqualified. The coordinator ../evidence/package fixture contains variant Flag and Holder(flag: Flag): interpreter prints42; native build fails CGXC005/E0425 for cgx2_user_4_Flag while the declared enum uses app.main.Flag.

Use the already checked canonical representation consistently for package record names and field types. Audit nested record/variant/Option/Result applications and imported qualified types for the same mismatch. Preserve canonical defining-module identity across colliding names. Do not infer type identity from basenames or patch only the one fixture. Preserve existing scalar/record Python behavior (Python is read-only here) and memory/foreign native routes.

## Acceptance
- Package source/native parity in debug and release for local record-of-variant, record-of-record, variant-of-record, Option/Result of local and imported aggregate field types, two-module same-name collisions, plus explicit nullary/payload patterns.
- Existing nominal negative rejection stays in sema; no lowering acceptance of unchecked/unknown types. Add an emission assertion demonstrating full canonical field type identities rather than only output42.
- Run cgx2-emit-rust tests and all-target/all-feature Clippy -D warnings; run the new cgx2c target and existing s2a_parity/package record regressions read-only. Format owned files and git diff --check. Record actual source/native outcomes, not only compilation success. Preserve interfaces for concurrent match-flow repair.

## Shared rules

Read ../README.md, ../../README.md, ../../contracts.md, ../../worker-rules.md, AGENTS.md and the design/validation requirements. This is a bounded repair of staged S2a, not approval of its interfaces for S2b/S3. Work only in your attached worktree on a riley/ branch. Preserve all public AST/type/value shapes, schemas, admission contracts and unrelated semantics. Root Cargo.toml/Cargo.lock, other lanes, shared docs/plans and coordinator evidence are read-only. No installs, paid APIs, host security changes, new chats, delegation or release actions.

Create private CARGO_TARGET_DIR=$PWD/target/release-s2a-REPAIR and TMPDIR=$PWD/target/tmp-s2a-REPAIR, substituting your lane name; set CARGO_BUILD_JOBS=2 and RUSTUP_TOOLCHAIN=1.98.1. Use LIBRARY_PATH=/tmp/cgx2-wave22-lib if needed. Do not share targets or measure release performance. Use existing offline dependencies. Record precise commands/results and limitations in your owned report, commit and stop for review. Source fixes must preserve existing positive support and reject invalid code before backend publication.
