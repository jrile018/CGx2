# Wave 4: memory-first parallel implementation

Start all five chats from the same `wave4/integration` commit created after Wave 3 was integrated and verified. This is a tactical split with an expected 15–25% coordination and review cost. Worktrees are user-owned; each worker commits its own bounded result and stops. No worker merges to `main`, starts another chat, schedules continuation, or messages another worker.

| Lane | Exclusive write surface | Stable input from Wave 3 | Reviewable output |
| --- | --- | --- | --- |
| [Memory frontend](memory-frontend/plan.md) | `crates/cgx2-syntax/**`, `cgx2-sema/**`, `cgx2-ownership/**`, `cgx2-cli/**`, `cgx2-emit-python/**`, `cgx2-api-index/**` | Existing AST, diagnostics, ownership `Op` IR | Parse/format/check source memory forms and lower a bounded safe subset to the ownership checker; CLI `check` diagnoses errors and `run` refuses unsupported memory execution |
| [Runtime report](runtime-report/plan.md) | `crates/cgx2-runtime/**` | `Session`, region stats, `no_gc` guards | Deterministic per-region memory report and guard/collection conformance tests |
| [Notebook kernel](notebook-kernel/plan.md) | New `crates/cgx2-notebook-kernel/**` only | `.cgnb` format/session and checked scalar CLI library | Pure-scalar cell execution, dependency tracking, restart/run-all with honest limitations |
| [Python bridge](python-bridge/plan.md) | `crates/cgx2-python-bridge/**` | Existing CPython/PyTorch handle API | Typed tensor multiply/matmul and real-environment capability probe |
| [Benchmark pilot](benchmark-pilot/plan.md) | `bench/**` only | Existing benchmark schemas/runner/report and scalar CLI | Executable CGx2/Python parity corpus and on-demand runtime pilot, without fabricated LLM claims |

## Shared contract and integration rules

- The Wave 3 commit is the frozen input. The memory lane owns **every existing crate with exhaustive AST matches**, so its parser changes can keep the entire workspace compiling without touching another lane's files. The other four lanes do not modify syntax, sema, ownership, CLI, Python emitter, or API index.
- The memory lane adds source memory constructs deliberately, with parsed spans and canonical formatting. It does not claim source programs execute memory operations until the CLI, runtime, and ownership paths are connected. `cgx2 run` must fail clearly for supported syntax it cannot execute. Python emission must reject memory constructs rather than change their meaning.
- The notebook kernel consumes the Wave 3 scalar execution API. It may replay earlier pure cells to preserve visible bindings, but it must cap work and reject unsupported effects or direct rerun semantics it cannot model. It must not claim a graphical Jupyter replacement.
- The Python bridge builds against the selected PyO3 interpreter. Tests with a real PyTorch installation are conditional; a missing `torch` is a reported missing acceptance gate, not success. No worker installs software into the user's system.
- The benchmark pilot uses executable parity oracles and records actual runs. Source length is only a descriptive measure; actual LLM token/use metrics require a model trace. No benchmark win can be claimed from a six-task pilot.
- Root `Cargo.toml`, `Cargo.lock`, `README.md`, shared docs, roadmap, and `AGENTS.md` are coordinator-owned. New crates are discovered by `members = ["crates/*"]`. Workers use offline dependencies already present or report a blocker. Each worker runs its package tests, format and Clippy where applicable, then commits only owned paths.

## Next integration gate

After the user reports all five chats complete, inspect exact commits and path ownership, integrate on `wave4/integration`, run the full workspace Rust tests (excluding the Python bridge unless its linker setup is available), benchmark tests, formatting, Clippy, scalar CLI smoke, and the new memory/notebook/Python gates. Review runtime semantics and source diagnostic behavior together. If the components are still disconnected, record that fact and plan a vertical integration wave before calling memory support complete.
