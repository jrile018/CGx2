# CGx2 S2b emitted target parity repair

Status: ready for bounded repair; foundation is not accepted.

## Exclusive ownership

crates/cgx2-emit-rust/** and crates/cgx2-emit-python/**

## Deliverable and acceptance

Repair two proven finite target failures without changing syntax/sema/public representations.
1. Rust builtin Option/Result declaration discovery scans typed.expressions but omits per-specialization typed.bodies. Generic size<T> using local get([value],0), returning length([value]), called at i64 and Bool interprets/Python-runs 2 but emitted Rust lacks Option<i64> and fails rustc --edition=2021. Collect all required concrete local nested types deterministically from existing checked specialization metadata. Cover multiple call orderings, local-only Option/Result, nested aggregates and generic contracts. Execute native debug/release and standalone emitted Rust.
2. concat("\x41","") passes source checking, Rust emitter panics at expect("validated string"), interpreter rejects CGR009 and Python emits raw literal yielding A. Validate all reachable finite-path string literals before output, including generic bodies and contracts; never panic or publish artifacts on invalid literals. Decode/requote Python strings with CGx2 semantics. Existing allowed escapes are newline n, carriage return r, tab t, backslash and double quote; preserve literal Unicode and do not add host-only x/u/octal semantics. Keep existing rejection classes/source spans where possible. Both emitters must reject unsupported escapes before replacing output. Cover supported escapes, quotes/backslashes, non-ASCII, invalid escapes and nested data/generic paths. Do not require a sema change: each emitter must be independently safe on the current checked module contract.
Repros: ../evidence/local-option.cg, native-local-option.json, bad-escape.cg and additional-report.json. Tests must exercise actual emission/execution rather than snapshots alone. Preserve other target unsupported outcomes.

## Execution rules

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/README.md and REVIEW.md, plans/release-completion/README.md, contracts.md, worker-rules.md, s2b-foundation/plan.md, s2b-review/README.md and this plan before coding. Follow MegaMemory overview/query/record and Serena initial instructions.

Work only in your attached worktree on a riley/ branch. Complete this bounded repair yourself; do not delegate, create chats or launch follow-on work. All paths outside exclusive ownership are read-only, including root Cargo.toml/Cargo.lock, shared syntax/type definitions, shared docs, foundation evidence and coordinator plans. Preserve TypedModule, Type, ResolvedCall, imported summary shapes, specialization keys and source admission contracts. Do not introduce dependencies or change public schema versions. Missing cross-owner changes must be reported precisely, not edited.

Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=$PWD/target/release-LANE and TMPDIR=$PWD/target/tmp-LANE (replace LANE with your lane; create directories). LIBRARY_PATH=/tmp/cgx2-wave22-lib if needed. No shared target directory, package installation, external paid calls, host security changes or performance measurements.

Run owned package tests with --all-features --offline --locked, owned all-target/all-feature Clippy --offline --locked -- -D warnings, owned formatting and git diff --check. Build cgx2-cli/cgx2c in your private target when product checks need them. Execute applicable source/native debug/release/Python cases outside repository-relative assumptions. Preserve existing artifact on rejected emit/build. Record exact commands, outcomes and remaining cross-lane failures in an owned REPAIR-S2BF.md plus reproducible tests/fixtures. Do not weaken checks or count known other-lane failures as passes. Coordinator runs full combined workspace and product gates after integration. Commit and stop; report commit, files, checks and limits.
