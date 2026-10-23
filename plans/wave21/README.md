# Wave 21: structured values and measured interoperability

The six coding chats start from the same `wave21/integration` plan commit, based on reviewed Wave 20 integration `3478da2`. Before coding, read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and your assigned `plan.md`. Call the project knowledge graph overview and query first; record the result after the task. Use only the worktree attached to your chat and the exclusive paths below. Deliver one bounded change, run its specified checks, commit it, and stop for the user's review. Do not merge to `main`, automate follow-on work, create more chats, edit another lane, or edit root Cargo files or shared design/progress/roadmap documents. The coordinator handles root `Cargo.lock` updates.

| Lane | Exclusive write surface | Bounded result |
| --- | --- | --- |
| [Region transfer and alias evidence](region-transfer-assurance/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**`, `crates/cgx2c/tests/memory_release_*` | Cross-region, alias, and close-path conformance with an explicit safety ledger |
| [Nominal scalar records](nominal-scalar-records/plan.md) | Core syntax/checker/CLI/Rust emitter/API/effect/package crates, `crates/cgx2c/src/**`, `crates/cgx2c/Cargo.toml`, `crates/cgx2c/tests/record_*` | One readable immutable record profile with interpreter/native parity |
| [Direct float32 CPU matrices](direct-f32-matrix/plan.md) | `crates/cgx2-bind/**`, `crates/cgx2-libtorch/**` | Additive pinned LibTorch float32 rank-2 route without Python |
| [Pure local imports in notebooks](notebook-pure-imports/plan.md) | The five `crates/cgx2-notebook-*` crates | Reproducible `.cgnb` calls to checked local pure scalar package functions |
| [Python-only numeric array bridge](python-numeric-arrays/plan.md) | `crates/cgx2-python-bridge/**`, `crates/cgx2-foreign/**` | Explicit copied float64 array boundary for declared Python-only calls |
| [Real matrix benchmark baselines](matrix-runtime-baselines/plan.md) | `bench/**` | Reproducible source/direct-adapter versus C++/Python Torch latency and resource evidence |

## Interface and sequencing rules

The record lane may extend the AST and checker but must preserve existing scalar, tensor, package, and notebook APIs. The memory lane owns only files whose names begin `memory_release_` under `cgx2c/tests`, and cannot edit `cgx2c` production code. The notebook lane consumes the already committed Wave 20 acyclic scalar package interface; it must not depend on the record lane's unfinished syntax or change `cgx2-package`. The float32 lane adds a versioned route and must keep the Wave 20 `cgx2-libtorch-facade-v1` ABI, `tensor-f64-cpu-matrix-v2` layout, symbols, and ownership behavior intact. The Python numeric-array lane is for Python-only libraries; the direct LibTorch path remains the PyTorch route. The benchmark lane is read-only toward compiler/foreign crates and must never synthesize a measured success from a mock.

The local host still has no executable Z3/CVC5, no genuine paired LLM telemetry, and no delegated CPU/memory/PID cgroup controllers for scored isolation. A worker may document a skipped external test but must not count it as a pass. Wave 21 work does not automatically change the 92% coarse implementation inventory or close the memory release gate.

## Coordinator review gate

When the user reports completion, inspect exact commits, ownership, and interface drift; integrate on a review branch; update root lockfiles; run offline workspace Rust tests, notebook UI loopback, Python bridge tests, benchmark tests/readiness, format and Clippy, record interpreter/native parity, source/native memory cases, and pinned real LibTorch routes where packages exist. Check raw benchmark measurements and external-tool exclusions before changing the progress estimate. Keep `main` unchanged until the user reviews the integrated result.
