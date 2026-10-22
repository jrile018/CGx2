# CGx2 notebook file command

`cgx2-notebook` executes a version 1 `.cgnb` file from a clean pure-scalar session. It accepts immutable `let` bindings and pure scalar expressions, and saves fresh `text/plain` outputs in file order. Saved code outputs are stale cache and are removed before execution. Markdown source is retained verbatim.

From the repository root:

```sh
cargo run -p cgx2-notebook-cli --offline --bin cgx2-notebook -- inspect crates/cgx2-notebook-cli/examples/two-cells.cgnb
cargo run -p cgx2-notebook-cli --offline --bin cgx2-notebook -- run crates/cgx2-notebook-cli/examples/two-cells.cgnb --out /tmp/two-cells-executed.cgnb
cargo run -p cgx2-notebook-cli --offline --bin cgx2-notebook -- inspect /tmp/two-cells-executed.cgnb
```

`inspect` lists each cell ID, kind, and clean-session status without executing code. Saved output is `cached` when provenance matches the source and code prefix, or `stale` when they differ; legacy output without provenance is `cached`. It prints provenance fields when present. `run` never changes the input unless the same path is explicitly named with `--out` and `--force`. An existing output requires `--force`; a failed cell or file operation does not replace it. The save uses a temporary sibling file and atomic replacement where the platform permits.

`export INPUT.cgnb --out PORTABLE.cgnb` bundles referenced sidecar assets with the notebook and preserves provenance. `import PORTABLE.cgnb --out OUTPUT.cgnb` verifies embedded bytes and restores `OUTPUT.cgnb.assets` before writing the native notebook. Both accept `--force` for an existing output. A portable file must be imported before `inspect` or `run`.

Input asset outputs are validated against the sidecar directory named `INPUT.cgnb.assets`, including size and SHA-256. The clean pure-scalar run drops those stale rich outputs and leaves the sidecar unchanged. Text in Markdown cells, including asset links, is retained; relative links may need adjustment if the new notebook is saved in another directory. This command does not provide a live UI, individual side-effecting reruns, Python/PyTorch execution, or arbitrary CGx2 source support. The kernel's cell count, source size, replay, and scalar evaluator limits apply.
