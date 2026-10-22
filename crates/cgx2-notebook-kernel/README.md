# CGx2 pure scalar notebook kernel

`NotebookKernel` executes `.cgnb` cells in a process-local scalar session. The supported CGx2 subset is immutable `let` bindings and pure scalar expressions (checked i64 arithmetic, booleans, and strings). A final non-Unit expression becomes captured `text/plain` output. Calls, mutable bindings, functions, modules, loops, Python, I/O, and arbitrary host commands are unsupported; the kernel never executes notebook HTML or script.

`run_cell(id)` executes the next code cell in file order. `replace_cell(id, source)` marks that cell and its transitive readers stale. `rerun_affected(id)` runs the edited cell and only those stale readers, preserving independent outputs and execution revisions. `rerun_cell(id)` remains available to run just the selected cell. Source dependencies must point to earlier cells; a forward dependency or cycle produces `CGNK009`. A required earlier binding that is not current produces a prerequisite diagnostic. `restart_and_run_all()` clears retained state, marks prior outputs stale, then executes every code cell once. If execution stops, prior text remains visible as a stale observation.

Each captured text body is at most 65,536 UTF-8 bytes and has schema 2 provenance with its cell ID, execution revision, source SHA-256, transitive dependency SHA-256, evaluator profile, and output ordinal. Saved output reopens as cached or stale observation without executing cells. Reopening never restores live bindings. The kernel limits code to 128 cells, 64 KiB of cumulative source, and 1 MiB of cumulative scalar evaluation script and retained state per clean session.

**Effect boundary:** no `.cg` effectful cell is admitted by this scalar worker. A final pure expression yields a captured display value; that capture is the only persisted observation. Calls, including potential file, network, and foreign operations, fail with `CGNK011` before evaluation. Other unsupported syntax fails with `CGNK001`. The frozen `check_source`/`run_source` scalar path does not provide an isolated, replay-safe effect protocol or effect receipt, so claiming persistent effectful execution would be unsound. Opaque cells and all later saved outputs reopen stale even if their source hashes match. This is not Jupyter ecosystem parity.

| Event | Selected cell | Later dependent output | Independent output |
| --- | --- | --- | --- |
| Successful run | current, with new provenance | stale after a rerun or edit until recomputed | retains current revision |
| Edit | stale, previous text retained | stale, previous text retained | retains current revision |
| Error, timeout, or acknowledged interrupt | failed, previous text retained | stale, previous text retained | retains current revision |
| Restart | stale if previously observed | stale if previously observed | stale if previously observed |
| Reopen saved file | cached only for matching pure provenance | stale when affected or following an opaque cell | cached only for matching pure provenance |

```rust
use cgx2_notebook_format::{parse_notebook, write_notebook};
use cgx2_notebook_kernel::NotebookKernel;
let source = "#!cgnb 1\n%% code id=seed\nlet x = 3\n%% code id=result\nx + 4\n";
let mut kernel = NotebookKernel::open(parse_notebook(source)?)?;
kernel.restart_and_run_all()?;
kernel.replace_cell("seed", "let x = 5".into())?;
kernel.rerun_affected("seed")?;
assert!(write_notebook(kernel.notebook())?.contains("mime=text/plain"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

`restart()` discards retained bindings, clears an outstanding interrupt request, and marks saved code outputs stale without evaluating. `restart_and_run_all()` then executes file order from the clean state. Reopening a saved `.cgnb` never evaluates. `set_execution_control(ExecutionControl::new(worker_executable, timeout))` runs each scalar statement in a killable child process; the default CLI/UI timeout is 2 seconds per cell, measured from the start of `execute_named`. The worker is polled every 5 ms and killed on timeout or an `interrupt()` signal. `CGNK010` identifies interruption, timeout, or worker failure. The parent only commits pending bindings after the whole cell succeeds, so an interrupted cell cannot become current. The bounded subprocess covers scalar evaluation; in-process parse, dependency extraction, and static checking use the existing 64 KiB source limit and are not preemptible by the timeout. The UI interrupt response reports `interruptRequested: true` when it sets the signal; the cell becomes failed only after the worker stops and the run request returns. No external side effects are supported by this scalar subset.
