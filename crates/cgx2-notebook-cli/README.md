# CGx2 notebook command

The [function lifecycle example](examples/function-lifecycle.cgnb) defines `inc`
in one cell, calls it through `twice` in another, and keeps an independent
result. Run and reopen it with:

```sh
cgx2-notebook run crates/cgx2-notebook-cli/examples/function-lifecycle.cgnb --out /tmp/functions-run.cgnb
cgx2-notebook inspect /tmp/functions-run.cgnb
cgx2-notebook delete /tmp/functions-run.cgnb base --out /tmp/functions-deleted.cgnb
cgx2-notebook inspect /tmp/functions-deleted.cgnb
```

After deletion, `derived` and `value` are stale saved observations; the
independent result stays cached across reopen. `delete` writes a new notebook
and does not execute cells. A missing call reports which pure function must be
defined or rerun. Current persistent notebook support is limited to pure
scalar functions (`i64`, `Bool`, `String`), immutable scalar bindings, and
bounded calls. Effectful persistent state, arbitrary imported libraries, and
Python/Jupyter execution are not supported.

`cgx2-notebook` reads native `.cgnb` files and executes only pure scalar CGx2 cells. A final expression produces captured `text/plain` display; immutable `let` bindings may feed later cells. HTML and scripts in saved text remain inert. The notebook format limits one captured text output to 65,536 UTF-8 bytes, a document to 16 MiB, and the kernel to 128 code cells, 64 KiB of cumulative source, and 1 MiB of scalar evaluation state.

```sh
cgx2-notebook run experiment.cgnb --out executed.cgnb
cgx2-notebook run experiment.cgnb --out revised.cgnb --edit-cell seed --source 'let x = 5'
cgx2-notebook inspect revised.cgnb
cgx2-notebook show revised.cgnb result
cgx2-notebook export revised.cgnb --out portable.cgnb
cgx2-notebook import portable.cgnb --out reopened.cgnb
```

`run` starts a clean evaluator and executes code cells in file order. With both `--edit-cell ID` and `--source CG_CODE`, it then edits that cell and reruns only the edited cell and its transitive readers in the same process. Independent cells keep their output and execution revision from the clean run. This form provides a CLI selective-rerun workflow without trusting saved bindings as live state. A separate command invocation cannot resume an earlier process's bindings; `inspect` and `show` read saved output and provenance without executing cells. `show` escapes control characters in display text. Reopening preserved output is cached or stale, never current until execution.

`run` and `show` work on ordinary and portable `.cgnb` input. `export` embeds verified referenced PNG, JPEG, GIF, or WebP assets; `import` writes ordinary notebook text and verified sidecar assets. Neither command changes the input unless that path is explicitly used with `--out` and `--force`. Existing destinations require `--force`; failed validation does not replace them. Notebook cells cannot access external files, networks, host commands, Python, or arbitrary CGx2 effects. This command does not claim Jupyter plugin parity.

`cgx2-notebook restart INPUT.cgnb --out OUTPUT.cgnb` persists stale saved observations without executing. `run` accepts `--timeout-ms N` (1–60,000; default 2,000) for each cell's scalar worker phase. On `CGNK010`, `run` exits nonzero and writes the partial notebook to `--out`: completed earlier cells remain current in that run, the interrupted cell is failed, and dependent saved outputs are stale. The input is untouched unless selected as `--out` with `--force`. The child process is killed at the deadline; static parse/check and output serialization remain in process and are not wall-clock preemptible. A later invocation starts from a clean session and does not restore in-memory bindings from saved output.
