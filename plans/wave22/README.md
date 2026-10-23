# Wave 22: memory evidence and bounded source interoperability

All five coding chats start from the same verified Wave 22 plan commit. Before coding, read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and your assigned `plan.md`. Use the MegaMemory overview → query → record workflow. Work only in the worktree attached to your chat. Deliver one bounded change, run specified checks, commit only owned paths, report the SHA and limitations, and stop for the user's review. Do not merge, launch a follow-on wave, edit another lane, or change root Cargo files or shared design/progress/roadmap documents.

| Lane | Exclusive write surface | Bounded result |
| --- | --- | --- |
| [Region graph model and source unwind](region-graph-model/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**`, `crates/cgx2c/tests/memory_release_*` | New failure-state model and source/native parity evidence |
| [Source float32 CPU matrix call](source-f32-matrix/plan.md) | `crates/cgx2-sema/**`, `crates/cgx2-cli/**` | Checked `.cg` call into the pinned direct float32 facade |
| [Immutable record notebook cells](notebook-record-cells/plan.md) | Five `crates/cgx2-notebook-*` crates | Pure record definitions, bindings, saved state, and selective rerun |
| [Portable scalar record emission](python-record-emission/plan.md) | `crates/cgx2-emit-python/**` | Standalone immutable record subset in checked Python output |
| [Larger CPU matrix baselines](matrix-compute-baselines/plan.md) | `bench/**` | Larger warm real float64 LibTorch and C++ comparison |

## Frozen interfaces and independence

The memory lane consumes the committed `Session`, root census, region reports, source `memory.*` profile, and `cgx2c` debug/release output. It may add only `cgx2c` tests whose filenames start `memory_release_`. It must label direct-runtime-only graph operations and must not claim a finite model is a general safety proof. The source float32 lane consumes the `cgx2-libtorch-facade-v1` ABI, `tensor-f32-cpu-matrix-v3` layout, `cgx2_torch_matmul_f32` export, and distinct float32 Rust handle API; it must not change binding, LibTorch, syntax AST, or package crates. The float64 `tensor-f64-cpu-matrix-v2` source route remains valid. Notebook and Python emitter lanes consume Wave 21 record semantics without editing core syntax/checker/CLI. Notebook imported package APIs remain pure scalar, and Python record emission is standalone only. The benchmark lane uses committed float64 routes and cannot depend on concurrent float32 source work. Source matrix literals are limited to 32 rows/columns and 1,024 elements; larger warm adapter cases need separate labels.

Every lane preserves source-located diagnostics, deterministic failure classes, and existing scalar behavior. Root `Cargo.lock` belongs to the coordinator. Workers do not update the progress estimate. LeakSanitizer ptrace, live Z3/CVC5, matching Python Torch, and delegated cgroups may remain unavailable; a skip is not a pass. The 92% tracked implementation estimate does not imply release readiness or a performance/LLM win.

## Coordinator review gate

After the user reports completion, inspect exact commits and path ownership, integrate on a separate review branch, run combined offline Rust/UI/Python benchmark tests, format and Clippy, source/native memory and record/notebook cases, and real pinned Torch routes where available. Check raw samples and isolation before performance claims. Keep `main` unchanged pending user review.
