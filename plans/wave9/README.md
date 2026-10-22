# Wave 9: source contracts and release evidence

Five user-owned chats start from the same reviewed `wave9/integration` commit. This is a tactical code split with an estimated 20–25% coordinator review and integration cost. The core-language lane is deliberately wide because syntax, static checking, and source execution share one changing contract. The other four lanes consume the frozen Wave 8 APIs. Each chat commits its owned paths and stops; no worker merges to `main`, messages another worker, or automates later work.

| Lane | Exclusive write surface | Input and reviewable result |
| --- | --- | --- |
| [Source effects and contracts](source-contracts/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**` | Existing scalar source/check/run API; explicit source signatures plus checked bounded runtime contracts |
| [Memory assurance audit](memory-audit/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Stable `Session` signatures; larger deterministic and targeted memory safety evidence |
| [Declared Python calls](python-foreign/plan.md) | `crates/cgx2-python-bridge/**`, new `crates/cgx2-foreign/**` | Existing CPython bridge; a validated, allowlisted foreign-call contract and callable host API |
| [Notebook provenance](notebook-provenance/plan.md) | `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-ui/**`, `crates/cgx2-notebook-cli/**` | Existing pure-scalar `.cgnb` rerun; truthful saved output provenance and notebook export/inspection |
| [LLM/runtime holdout harness](benchmark-holdout/plan.md) | `bench/**` | Frozen Wave 8 compilers; runnable paired task protocol and measured/calibrated reports without invented model results |

## One-card interface contract

- Every lane starts from the identical Wave 8 integration commit. Only the core lane changes AST, parser, checker, evaluator, diagnostics, or API-index. Other lanes cannot depend on its new syntax or edit those paths. The Python lane produces a host API for a later `.cg` adapter; it does **not** claim Python calls work in source this wave. The notebook lane retains the pure-scalar evaluator and does not call a new core-language feature.
- `cgx2-runtime::Session` public signatures and existing error/collection semantics stay stable. Core contracts cannot weaken memory checks; the memory lane cannot add a public runtime API that the core lane needs before integration. The benchmark lane treats tools as black boxes and keeps fixture/source changes under `bench/**`.
- Root `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/**`, `plans/progress.md`, and `plans/roadmap.md` belong to the coordinator. `crates/*` workspace membership includes the new foreign crate automatically. Workers run their targeted offline tests, fmt, and Clippy, then commit only owned paths.
- Coordinator reviews changed paths and claims, cherry-picks onto the isolated integration branch, runs combined Rust/Python tests, local UI HTTP tests, bridge tests, source/contract and foreign-call smoke, memory replay, benchmark parity, fmt and Clippy. A failing acceptance test blocks credit until repaired. `main` remains untouched until explicit review.

## Dependency gate after Wave 9

Source Python/PyTorch calls, native managed-memory codegen, trusted SMT proofs, persistent side-effecting notebooks, general CPU/GPU planning, and OCaml-scale maturity remain separate next-stage work. Wave 9 does not clear the first-release memory hard gate by finite testing alone, and it cannot claim LLM superiority without real paired model traces and held-out correctness results.
