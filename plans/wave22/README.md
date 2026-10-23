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


## Coordinator review, 2026-10-23

The five clean worker worktrees each contain one commit descended from common plan `5815b4b`. All changed paths match their assigned ownership. They were cherry-picked onto `riley/wave22-review` in the attached coordinator worktree; `main` and worker histories were not changed.

| Lane | Worker commit | Reviewed integration commit |
| --- | --- | --- |
| Region graph model | `7bd6ed8` | `7ecd418` |
| Source float32 matrix | `d82b10e` | `f2ab495` |
| Notebook record cells | `3dc7275` | `93d537f` |
| Python record emission | `7427ac5` | `65b6ea5` |
| Larger matrix baselines | `59d26ce` | `9172cf4` |

Coordinator fix `c978c48` repairs two accepted-source edge cases: a Python record field named `self` no longer collides with the generated constructor receiver, and notebook record transport accepts the Unicode identifiers accepted by the lexer. Regression tests cover reserved Python field names and Unicode worker/retained-state/save-reopen behavior. The public pilot `compiler_source_git_tree` is now `7f49e8e0f5ebbb322422315dae822615f495b94a`; compiler tests changed the tree even though the production compiler did not change in this wave.

### Verification and evidence limits

- The combined offline Rust workspace suite excluding the socket UI passed. After the coordinator fixes, all Python-emitter and notebook-kernel tests passed, as did the UI suite including 18 HTTP tests. Workspace Clippy with `-D warnings`, `cargo fmt --all --check`, and `git diff --check` passed.
- All 108 Python benchmark tests passed after refreshing the pilot pin. `python3 bench/readiness.py --build` built the integrated compiler and passed six public reference tasks. Delegated cgroups remain unavailable; independent human task review and genuine paired model traces remain unrun.
- Commands used Rust 1.98.1, `LIBRARY_PATH=/tmp/cgx2-wave22-lib`, `CARGO_TARGET_DIR=/home/john-riley/projects/lang/target`, and `TMPDIR=/home/john-riley/projects/lang/target/cgx2-wave21-tmp` to avoid this host's `/tmp` quota. Coordinator logs are `target/wave22-workspace-test.log`, `wave22-regression-test.log`, `wave22-ui-test.log`, `wave22-clippy.log`, and `wave22-readiness.json` in the original project checkout.
- A separate real source test used `CGX2_LIBTORCH_F32_SOURCE_PACKAGE=/home/john-riley/projects/lang/target/cgx2-wave22-f32-source` with `cargo test -p cgx2-cli --test tensor real_pinned_float32_matrix_source_when_available --offline -- --exact --nocapture`. The cloned, pinned Torch 2.11 CPU v3 package passed checked source execution and the 2x3 by 3x2 numeric reference within `1e-5` in 198.27 seconds. Its log is `target/wave22-real-f32.log`.
- **The no-mapped-Python requirement is not satisfied by the current CLI.** `ldd target/debug/cgx2` shows `libpython3.14.so.1.0` loaded at process startup through CLI → foreign → Python bridge → PyO3. The LibTorch operation makes no Python call or fallback, but that is insufficient to claim a Python-free executable. Wave 23 explicitly addresses this boundary; the passing numeric test does not close it.
- The memory lane's four-seed replay independently checks region graphs and failure transitions; its auto region uses `trigger_bytes=4096` and does not establish collection under allocation pressure. The worker reports normal and fault-injection suites twice, source/native parity, and AddressSanitizer on the direct runtime model. LeakSanitizer could not inspect leaks on this host. The instrumentation excludes uninstrumented foreign dependencies and does not prove general memory safety.
- The [raw matrix report](../../bench/results/wave22-matrix-runtime.json) is valid with no failed runs. Coordinator recomputation matches its summary and confirms 20 warm samples per C++/facade route at 128x128 and 512x512. Warm facade/C++ medians are 85,961/94,491 ns and 4,387,887/4,715,306 ns; p95 ratios are approximately 1.050 and 0.970. Each cold source route has only one sample, about 156.65 and 157.84 seconds. These source binaries use the debug Cargo profile, while the C++ baseline uses Release; the observations are engineering data, not a fair optimized source-runtime comparison. Warm direct C-facade measurements omit the Rust pinned-loader checks. Python Torch is unavailable, and inference/whole-language throughput targets remain unscored.

The tracked first-release estimate remains **92%**. Local record cells, standalone Python records, and bounded float32 source calls are implemented, but release memory safety, Python-free native CLI execution, general semantics and library APIs, trusted live proof, scored isolation, and paired LLM evidence remain open. Next: [Wave 23](../wave23/README.md).
