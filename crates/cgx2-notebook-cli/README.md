# CGx2 notebook command

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
