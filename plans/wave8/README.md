# Wave 8: run modular code and harden runtime decisions

Five user-owned chats start from one verified `wave8/integration` commit. This is a tactical parallel split with an expected 15–25% coordinator review/integration cost. Each chat commits only its exclusive paths and stops for user review. No automation, cross-worker messaging, or merge to `main`.

| Lane | Exclusive write surface | Frozen Wave 7 input | Reviewable result |
| --- | --- | --- | --- |
| [Native package execution](package-native/plan.md) | `crates/cgx2-package/**`, `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Acyclic checked local package graph, scalar Rust target | `cgx2c build --package ROOT --out BIN` runs bounded imported scalar functions |
| [Memory state-machine fuzz](memory-fuzz/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Wave 7 conversion-aware model, stable `Session` API | Reproducible randomized direct-runtime operations and minimized failures |
| [Notebook selective rerun](notebook-rerun/plan.md) | `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-ui/**` | Pure-scalar `.cgnb` format and local UI | Rerun a selected pure cell with correct invalidation and save behavior |
| [Measured CPU auto policy](cpu-auto/plan.md) | `crates/cgx2-execution-policy/**` | Reviewed checked i64 map and raw calibration | Conservative local measurements can select parallel CPU with measured fallback |
| [Security policy preview](security-policy/plan.md) | `crates/cgx2-effect-index/**`, new `crates/cgx2-security/**` | Advisory local call/effect index | Source-located external effect policy and bounded taint findings, SARIF output |

## One-card interface contract

- Source parser, AST, sema, CLI, Python bridge, verifier, notebook format, and benchmark harness are frozen in Wave 8. The native-package lane may extend only package/emitter/compiler APIs; it must preserve standalone `cgx2c build FILE.cg` and `cgx2-package::load_package`. No sibling edits those paths. The security lane consumes the frozen Wave 7 syntax/package semantics and only changes effect-index/new security files; imported effects are `Unknown` unless it can resolve them soundly within its own files.
- The memory lane may repair runtime internals but preserves public `Session` signatures and source memory semantics. The notebook lane uses only existing pure-scalar evaluator behavior; side effects and Python/PyTorch remain unsupported. The CPU policy lane owns a source-independent reviewed map primitive; it cannot claim arbitrary `.cg` auto-parallelization or GPU execution.
- Root `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/**`, progress and roadmap files belong to the coordinator. The five write surfaces are disjoint. Workers run offline package tests, fmt and Clippy, report exact limits, and stop with a commit. Coordinator cherry-picks on a review branch, runs combined Rust/Python suites, local UI HTTP tests with loopback permission, Python bridge tests, package execution smoke, memory replay/fuzz seeds, policy validation, security output and benchmark parity.

## Dependency gate after Wave 8

Source-level effect/capability/contract declarations, Python/PyTorch calls inside `.cg`, native managed-memory codegen, persistent side-effecting notebooks, trusted solver proofs, and general GPU backends require shared syntax/typed-IR/runtime interfaces. They are not split across these Wave 8 chats. A later wave should allocate a wide core-language lane for those shared changes after package execution and memory evidence stabilize. The first-release hard memory gate and LLM/runtime outcome targets remain open.
