# CGx2 pure scalar notebook kernel

`NotebookKernel` executes `.cgnb` code cells in a process-local scalar session. Each successful immutable `let` retains its checked value and defining cell ID. `run_cell(id)` evaluates only the next code cell. The kernel uses the frozen Wave 11 `cgx2_cli::check_source` and `run_source` semantics on the selected cell and serialized scalar values; it never executes earlier cell source to run a later cell. Markdown is never executed.

```rust
use cgx2_notebook_format::{parse_notebook, write_notebook};
use cgx2_notebook_kernel::NotebookKernel;

let input = "#!cgnb 1\n%% code id=seed\nlet x = 3\n%% code id=result\nx + 4\n";
let mut kernel = NotebookKernel::open(parse_notebook(input)?)?;
kernel.restart_and_run_all()?;
let saved = write_notebook(kernel.notebook())?;
assert!(saved.contains("mime=text/plain ordinal=1\n7"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

`rerun_cell(id)` invalidates that cell and every later cell's retained definitions, then evaluates only the selected cell. This conservative file-order suffix rule prevents using definitions from an old prefix even when dependency facts are incomplete. The selected output becomes current; old later outputs stay visible and noncurrent until their cells run again. If a selected cell needs an earlier stale, failed, cached, or never-run definition, it returns `CGNK004` with the prerequisite cell ID; rerun that prerequisite first or use **Restart and Run All**. `replace_cell` applies the same suffix invalidation and marks affected old outputs stale. `run_cell` remains blocked after an edit until a selected rerun or clean restart. `restart_and_run_all()` discards old code outputs, clears evaluator state and the cumulative evaluation budget, then executes every code cell in file order. Saved `.cgnb` text, output ordinals, and provenance persist, but reopening starts every saved output as cached or stale, never current.

The supported source subset is immutable `let` bindings and pure scalar expressions: checked i64 arithmetic, booleans, strings, names, and supported scalar type annotations. Calls, functions, modules, loops, assignment, mutable values, qualified names, I/O, Python/PyTorch, native bindings, memory operations, and unknown effects are rejected before execution. A failed cell commits no definitions; its old output remains visible with `Failed` status and can be retried. This is not persistent side-effect support or full Jupyter parity.

Limits are 128 code cells, 64 KiB of cumulative code source, and 1 MiB of cumulative scalar evaluation script bytes per clean session, including selected reruns. Retained names, defining IDs, and values are also capped at 1 MiB. The public `MAX_REPLAY_BYTES` constant remains an alias for this evaluation limit. CLI evaluation has its own one-million-step limit. Error codes are `CGNK001` unsupported source, `CGNK002` cell count, `CGNK003` source bytes, `CGNK004` prerequisite/restart required, `CGNK005` out of order, `CGNK006` evaluation/state budget, `CGNK007` notebook/session misuse, and `CGNK008` checking or execution failure.
