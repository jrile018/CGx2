# CGx2 local notebook UI preview

From the repository root, run:

```sh
cargo run -p cgx2-notebook-ui --offline -- open crates/cgx2-notebook-cli/examples/two-cells.cgnb
```

Open the printed `http://127.0.0.1:PORT/` URL on the same computer. `--port PORT` selects a specific loopback port; by default the operating system chooses an available port. Press Ctrl+C in the terminal to stop. One process opens one named file. The server also stops after eight hours or 10,000 requests.

Code cells can be edited. Select **Apply edit** to update the in-memory session; **Run next** executes the next code cell in file order. An edit requires **Restart and run all** before further execution. Failed cells show a failed status and an error, never a successful output. Markdown source is displayed as inert, read-only text. Saved text output and source containing HTML-like text are also displayed as text. The browser loads only the crate's local JavaScript and CSS.

**Save as** writes a `.cgnb` file beside the opened notebook. Enter a simple filename with no directory separators. Existing files require the explicit **Overwrite existing file** checkbox, including when naming the opened file. A save serializes the current session with `write_notebook`, then writes a temporary sibling and replaces or creates the destination atomically. The opened file is unchanged until explicitly selected for overwrite. Current session text outputs are retained. Valid cached asset references on untouched, unrun cells are retained and their content-addressed sidecars are copied when saving to a new filename; the kernel discards other saved outputs when opening because they do not prove a fresh run. Reopened output is parsed before writing.

This is a pure-scalar preview: immutable `let` bindings and scalar expressions only. Python/PyTorch, memory scripts, shell commands, arbitrary `.cg`, persistent side effects, rich HTML execution, and Jupyter parity are outside this crate. Kernel limits include 128 code cells, 64 KiB of code source, and 1 MiB of cumulative replay. This UI additionally accepts at most 32 distinct saved assets totaling 64 MiB. The HTTP request body limit is 128 KiB and request reads have a five-second total deadline.

The server binds only `127.0.0.1`, checks exact loopback `Host` and `Origin`, requires a random per-process token for each change, and rejects unknown routes, oversized requests, and invalid JSON. It sends a restrictive Content Security Policy and `Cache-Control: no-store`. This protects the local UI from ordinary cross-origin requests; it does not protect against malicious processes or users with access to the same machine or browser profile.

Manual browser smoke on 2026-10-22 with a copy of `two-cells.cgnb` in `/tmp`: opened the printed URL and saw `seed` and `result` as never-run; clicked **Run next** twice and saw `7` in `result`; edited `seed` from `3` to `5`, clicked **Apply edit**, and saw both cells stale plus the restart warning; clicked **Restart and run all** and saw `9`; saved as `cgx2-ui-smoke-saved.cgnb` and saw the saved confirmation. `cgx2-notebook inspect` parsed the saved file; the opened copy remained byte-for-byte unchanged.
