# CGx2 notebook file command

`cgx2-notebook` executes a version 1 `.cgnb` file from a clean pure-scalar session. It accepts immutable `let` bindings and pure scalar expressions, and saves fresh `text/plain` outputs in file order. Saved code outputs are stale cache and are removed before execution. Markdown source is retained verbatim.

From the repository root:

```sh
cargo run -p cgx2-notebook-cli --offline --bin cgx2-notebook -- inspect crates/cgx2-notebook-cli/examples/two-cells.cgnb
cargo run -p cgx2-notebook-cli --offline --bin cgx2-notebook -- run crates/cgx2-notebook-cli/examples/two-cells.cgnb --out /tmp/two-cells-executed.cgnb
cargo run -p cgx2-notebook-cli --offline --bin cgx2-notebook -- inspect /tmp/two-cells-executed.cgnb
```

`inspect` lists each cell ID, kind, and clean-session status without executing code. Saved output is `cached` when provenance matches the source and code prefix, or `stale` when they differ; legacy output without provenance is `cached`. It prints provenance fields when present. `run` never changes the input unless the same path is explicitly named with `--out` and `--force`. An existing output requires `--force`; a failed cell or file operation does not replace it. The save uses a temporary sibling file and atomic replacement where the platform permits.

`export INPUT.cgnb --out PORTABLE.cgnb` bundles referenced sidecar assets with the notebook and preserves provenance. `inspect`, `run`, and `export` open portable files directly after the original sidecar is removed. `import PORTABLE.cgnb --out OUTPUT.cgnb` verifies embedded bytes, stages a new `OUTPUT.cgnb.assets` directory, and writes an ordinary version 1 notebook with sidecars. Both export and import accept `--force` for an existing output; import also requires it to replace an existing destination sidecar. Without `--force`, neither command overwrites the input or an existing output. Naming the input as `--out` requires `--force`. Temporary sibling files and staged asset directories are removed when validation or writing fails; a bad asset is rejected before destination changes.

```sh
cgx2-notebook export analysis.cgnb --out analysis-portable.cgnb
cgx2-notebook inspect analysis-portable.cgnb
cgx2-notebook import analysis-portable.cgnb --out reopened.cgnb
```

The portable extension keeps readable version 1 cells and adds verified embedded image blocks. Older version 1 parsers reject those blocks; import produces a file they can read with its sidecar. Image MIME is limited to PNG, JPEG, GIF, and WebP. HTML and SVG assets are rejected, and text output is never executed. There is no network fetch or Python/Jupyter conversion.

Ordinary input assets are validated against `INPUT.cgnb.assets`; portable input assets are validated from embedded blocks, including MIME, size, and SHA-256. The clean pure-scalar run drops those stale rich outputs and leaves the sidecar unchanged. Text in Markdown cells, including asset links, is retained; relative links may need adjustment if the new notebook is saved in another directory. This command does not provide a live UI, individual side-effecting reruns, Python/PyTorch execution, or arbitrary CGx2 source support. Each `run` starts a fresh evaluator and executes cells once in file order; `inspect` and reopening never restore live bindings. The kernel limits apply: 128 code cells, 64 KiB code source, 1 MiB cumulative scalar evaluation script bytes, and 1 MiB retained bindings. Calls, mutable values, I/O, Python, native bindings, memory operations, and unknown effects are rejected.
