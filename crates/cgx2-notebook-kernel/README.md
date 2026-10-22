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

Cells run in file order. `run_cell(id)` accepts only the next code cell; retrying a failed cell is safe because failed source is never appended. Direct rerun of an accepted cell and running after `replace_cell` return `CGNK004` until `restart_and_run_all()` rebuilds from a clean notebook. Editing a definition stales dependent outputs through `cgx2-notebook-session`. Restart clears old code outputs before the first cell, so a failure cannot leave an old result looking current. `status(id)` exposes `NeverRun`, `Current`, `Stale`, or `Failed`. Serializing `notebook()` preserves cell IDs, sources, text MIME outputs, and ordinals.

The accepted source subset is immutable `let` bindings and pure scalar expressions: i64 arithmetic, booleans, strings, names, and the CLI's supported scalar type annotations. Calls, functions, modules, loops, assignment, mutable bindings, qualified names, memory operations, Python/PyTorch, and unknown syntax are rejected. Source diagnostics are included in error messages. This is a library, with no live graphical UI or side-effecting cell semantics.

Limits are 128 code cells, 64 KiB of cumulative code source, and 1 MiB of cumulative replayed script bytes per clean session. Each CLI evaluation also has its own one-million-step limit. Rechecking and replaying from the beginning costs more as the accepted transcript grows. Errors use stable kernel codes: `CGNK001` unsupported source, `CGNK002` cell count, `CGNK003` source bytes, `CGNK004` restart required, `CGNK005` out of order, `CGNK006` replay budget, `CGNK007` notebook/session misuse, and `CGNK008` source checking or execution failure.
