# CGx2 pure scalar notebook kernel

This library executes `.cgnb` code cells through `cgx2_cli::check_source` and `cgx2_cli::run_source`. It replays accepted cells from the beginning for every execution so earlier immutable bindings remain visible. Markdown cells are never executed. A clean open discards saved code outputs because they are cached observations, not current results.

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

Cells run in file order. `run_cell(id)` accepts only the next code cell; retrying a failed cell is safe because failed source is never appended. `rerun_cell(id)` accepts any code cell ID, clears evaluator bindings, replays every preceding code cell in file order from its current source, then executes the selected cell. Replay is internal: preceding cells keep their visible outputs and statuses, while the selected cell receives a fresh output and later readers of its definitions become stale. A failed replay marks the selected cell `Failed`; its old output remains cached and noncurrent. Rerunning does not duplicate definitions because it starts from empty bindings. After a successful selected rerun, `run_cell` can continue with the following code cell. `next_cell_id()` identifies that cell. Editing a definition stales transitive readers through `cgx2-notebook-session`; `run_cell` remains blocked by `CGNK004` after an edit until selected rerun or clean restart. `restart_and_run_all()` clears old code outputs, resets the replay budget, and runs in file order. `status(id)` exposes `NeverRun`, `Current`, `Stale`, or `Failed`. Serializing `notebook()` preserves cell IDs, sources, text MIME outputs, and ordinals.

The accepted source subset is immutable `let` bindings and pure scalar expressions: i64 arithmetic, booleans, strings, names, and the CLI's supported scalar type annotations. Calls, functions, modules, loops, assignment, mutable bindings, qualified names, memory operations, Python/PyTorch, and unknown syntax are rejected. Source diagnostics are included in error messages. This is a library, with no live graphical UI or side-effecting cell semantics.

Limits are 128 code cells, 64 KiB of cumulative code source, and 1 MiB of cumulative replayed script bytes per clean session, including selected reruns. Each CLI evaluation also has its own one-million-step limit. Rechecking and replaying from the beginning costs more as the accepted transcript grows. The prefix is replayed only through the selected cell, never beyond it. This is safe only for the checked pure-scalar profile: no prior I/O, Python, memory operation, or other side effect can run. It does not implement persistent side-effecting notebook semantics. Errors use stable kernel codes: `CGNK001` unsupported source, `CGNK002` cell count, `CGNK003` source bytes, `CGNK004` restart required, `CGNK005` out of order, `CGNK006` replay budget, `CGNK007` notebook/session misuse, and `CGNK008` source checking or execution failure.
