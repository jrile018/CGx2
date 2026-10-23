# Wave 20: source integration and release evidence

All six coding chats start from the same Wave 20 plan commit on `wave20/integration`, which contains the reviewed Wave 19 integration. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and your assigned `plan.md` before coding. Call the project knowledge graph overview and query before reading source, then record the result after the task. Work only in the attached Git worktree and the exclusive paths below. Deliver one bounded change, run the specified checks, commit, and stop for the user's review. Do not merge branches, automate follow-on work, create more chats, edit another lane or shared design/roadmap files, or edit root Cargo files. Coordinator owns root lockfile changes.

| Lane | Exclusive write surface | Bounded deliverable |
| --- | --- | --- |
| [Source and native memory evidence](source-native-memory-evidence/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**`, `crates/cgx2c/tests/memory_release_*` | Deterministic source/native unwinding corpus and explicit release-gate evidence |
| [Source CPU matrix calls](source-cpu-matrix/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**` | Checked `.cg` 2D float64 matrix multiplication through existing pinned C++ facade |
| [Matrix facade lifetime assurance](matrix-lifetime-assurance/plan.md) | `crates/cgx2-bind/**`, `crates/cgx2-libtorch/**` | Hostile-input and repeated-lifecycle assurance for the frozen v2 CPU matrix ABI |
| [Portable notebook images](notebook-portable-images/plan.md) | The five `crates/cgx2-notebook-*` crates | Explicit digest-referenced markdown images survive import/export and clean sweep trials |
| [Python loop parity](python-loop-parity/plan.md) | `crates/cgx2-package/**`, `crates/cgx2-emit-python/**` | Translate bounded scalar range loops with matching Python execution limits |
| [Parallel map reliability](parallel-map-reliability/plan.md) | `crates/cgx2-execution-policy/**` | Deterministic cancellation/failure behavior and conservative measured CPU dispatch |

## Fixed interfaces and independence

The source matrix lane consumes the already committed additive matrix API: ABI `cgx2-libtorch-facade-v1`, layout `tensor-f64-cpu-matrix-v2`, and schema-2 binding metadata. The facade assurance lane may add checks and tests but must not change these symbols, layout IDs, or ownership contract while source integration is in progress. The memory lane may add only `memory_release_*` tests under `cgx2c/tests`; it may not edit `cgx2c` production code. The source matrix lane may read but not edit `cgx2-libtorch`, `cgx2-bind`, or `cgx2c`. Other write surfaces are disjoint.

Wave 20 is a set of bounded advances, not a claim that the first release is ready. Proof and LLM evidence remain honest gates: no local Z3/CVC5 executable, genuine paired model traces, or delegated CPU/memory/PID cgroup controllers are available on the reviewed host. Workers must report a skipped external test as skipped and must not replace it with a mock success. General access to arbitrary Python libraries remains a separate compatibility program; the source matrix route is direct LibTorch C++, with no Python middleman.

## Coordinator review gate

When the user reports completion, inspect each exact commit and ownership, integrate on a review branch, update the root `Cargo.lock` if needed, run combined offline workspace tests, notebook UI loopback tests, 102+ Python benchmark tests and readiness, formatter and Clippy, source/native memory and `.cg` matrix/Python parity cases, and the real pinned matrix package if available. Recheck scope claims against test evidence, update progress and roadmap, then plan the next wave. Keep `main` unchanged until the user reviews the integrated result.
