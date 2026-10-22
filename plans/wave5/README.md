# Wave 5: connect memory execution and widen the usable toolchain

All five chats start from the same verified `wave5/integration` commit after Wave 4 review. Expected coordination cost is 15–25%. Each chat owns a disjoint code surface, commits its bounded result in its own worktree, reports exact tests and limitations, and stops for user review. No automation, worker-to-worker messaging, or merge to `main`.

| Lane | Exclusive write surface | Stable Wave 4 input | Reviewable output |
| --- | --- | --- | --- |
| [Memory execution](memory-execution/plan.md) | `crates/cgx2-cli/**` | Memory syntax/checker and runtime APIs | Checked `.cg` memory scripts actually run; guarded collection, owned moves, and structured memory report verified end to end |
| [Rust source emitter](rust-emitter/plan.md) | New `crates/cgx2-emit-rust/**` | Frozen syntax AST and typed scalar summary | Readable, compiled Rust for a checked scalar profile with differential fixtures |
| [Notebook command](notebook-command/plan.md) | New `crates/cgx2-notebook-cli/**` | `.cgnb` format and pure-scalar kernel | `cgx2-notebook run` writes a clean executed notebook without clobbering source |
| [Package index](package-index/plan.md) | New `crates/cgx2-package/**` | Source parser, semantic checker, API index | Strict `cgx2.toml` subset, safe source discovery, deterministic per-module API index |
| [Execution policy](execution-policy/plan.md) | New `crates/cgx2-execution-policy/**` | Runtime/benchmark contracts and execution design | Correctness-first CPU scalar/parallel choice prototype with explicit GPU capability boundary |

## Contract boundaries

- Syntax, sema, ownership IR, runtime, notebook format/kernel, Python bridge, Python emitter, API index, and benchmark harness are frozen inputs for this wave. Only the memory execution lane edits `cgx2-cli`, so no other lane introduces a new CLI command there or changes the source AST.
- The memory execution lane connects only the Wave 4 memory source profile: script-level `memory.region`, `owned_bytes`, `move`, `alloc_bytes`, `borrow`/`borrow_mut`/`end_borrow`, `use`, `drop`, `collect`, `close`, and nested `no_gc`. Unsupported source forms remain diagnostic errors. Runtime errors must not be converted into successful output.
- Rust emission targets the same scalar subset as the Python emitter, not all CGx2 memory semantics. It rejects memory/effects it cannot preserve. The package index parses manifests and module declarations but cannot claim import resolution until `import` syntax exists. The notebook command exposes clean pure-scalar file execution, not a Jupyter-equivalent UI.
- The execution policy may make actual CPU scalar/parallel choices for a small reviewed pure primitive. GPU is selectable only with a real supported backend and validated semantics; this wave defines the capability and cost boundary but does not claim a GPU backend exists. `auto` chooses the known-correct sequential path when measurements/capabilities are absent. `no_gc` and ownership rules outrank estimated speed.
- Root `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/**`, `plans/roadmap.md`, and `AGENTS.md` are coordinator-owned. The wildcard workspace discovers new crates. Workers use offline dependencies or report a blocker. Every Rust lane runs package tests, fmt, and Clippy; memory execution additionally runs the full non-Python workspace suite.

## Review and next gate

After the user reports completion, inspect commits and ownership, cherry-pick on the isolated integration branch, run all combined suites and end-to-end memory/emit/notebook/package examples, and record missing first-release gates. Real PyTorch, source imports, effects/contracts, notebook UI, general automatic GPU execution, and statistically sound LLM/native runtime wins remain future work unless independently verified.
