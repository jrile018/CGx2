# S2a repair: match control-flow typing

## Exclusive ownership
- crates/cgx2-sema/**
- crates/cgx2-cli/tests/s2a_match_control_flow.rs (new file only)
- crates/cgx2c/tests/s2a_match_control_flow.rs (new file only)
- crates/cgx2-sema/S2A-MATCH-REPAIR.md

Do not edit existing cgx2-cli/cgx2c test files or any emitter source. The native repair owns the emitters; preserve checker signatures and type shapes.

## Confirmed defects and required result
1. check_match compares every arm tail to the enclosing function result, including a non-final match. The coordinator fixture ../evidence/nontail.cg assigns n=42 in the arm then returns n after the match; it incorrectly fails CGT003 expected i64 found Unit. Separate enclosing explicit-return expectations from tail-value expectations. A non-final match must not implicitly return from its function. Keep branch agreement and all explicit Return checks sound.
2. check_inferred_result::returns visits If but omits Match. ../evidence/inferred.cg has an inferred helper with an early Boolean return inside a match arm plus i64 tails. It passes check, fails interpreter CGR004 and native CGXC005/E0308. Reject inconsistent reachable return/tail types at source checking with accurate spans. All-return match helpers must infer correctly, including when unused; do not default them to Unit and generate invalid Rust.
3. Audit the same control-flow traversal for nested matches/ifs, matches within existing loops, wildcard/empty or no-value branches, annotated versus inferred functions, partial-return branches and explicit return None. Preserve unreachable-arm and exhaustive checking. Do not add new language features or silently weaken typing to make examples run.

## Acceptance
- Add semantic regression tests for all confirmed cases, valid non-final mutation match, valid non-final branches whose value differs from enclosing result, inferred all-return helper, nested/partial explicit returns, annotated mismatch, tail mismatch, and correct UTF-8 diagnostic spans.
- Product checks must reject inferred.cg before interpretation/native compilation, with no backend artifact. Valid non-tail and inferred all-return cases must agree in source and native debug/release execution. Put native parity in the exclusively owned new test file; fixtures may live under sema/tests.
- Run cgx2-sema tests and Clippy all-target/all-feature with -D warnings; run the two new product/native test targets plus existing S2a semantic/CLI/native tests read-only. Run formatting for owned files and git diff --check. Do not weaken any test. Broader integration is coordinator-owned.

## Shared rules

Read ../README.md, ../../README.md, ../../contracts.md, ../../worker-rules.md, AGENTS.md and the design/validation requirements. This is a bounded repair of staged S2a, not approval of its interfaces for S2b/S3. Work only in your attached worktree on a riley/ branch. Preserve all public AST/type/value shapes, schemas, admission contracts and unrelated semantics. Root Cargo.toml/Cargo.lock, other lanes, shared docs/plans and coordinator evidence are read-only. No installs, paid APIs, host security changes, new chats, delegation or release actions.

Create private CARGO_TARGET_DIR=$PWD/target/release-s2a-REPAIR and TMPDIR=$PWD/target/tmp-s2a-REPAIR, substituting your lane name; set CARGO_BUILD_JOBS=2 and RUSTUP_TOOLCHAIN=1.98.1. Use LIBRARY_PATH=/tmp/cgx2-wave22-lib if needed. Do not share targets or measure release performance. Use existing offline dependencies. Record precise commands/results and limitations in your owned report, commit and stop for review. Source fixes must preserve existing positive support and reject invalid code before backend publication.
