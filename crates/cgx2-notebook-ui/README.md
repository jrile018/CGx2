# CGx2 loopback notebook UI

Markdown `![alt](asset:sha256:<64 lowercase hex>)` references render through the token-protected asset route after digest and image-type validation. Source text is shown verbatim; the UI builds image elements with DOM APIs and blob URLs under its restrictive Content Security Policy. Other Markdown links remain text and trigger no file or network fetch.

For a parameter sweep, use `cgx2-notebook sweep INPUT.cgnb CELL_ID --value
LITERAL --out-dir DIRECTORY` from a terminal. Open an individual result with
`cgx2-notebook-ui open DIRECTORY/trial-01.cgnb`; repeat for other trials.
`summary.json` lists saved successes and failures. Opening a result only reads
its saved output and provenance; use **Restart and run all** to execute it
again. The UI does not launch sweeps or expose a new execution endpoint.

**Delete cell** removes the selected cell in memory. Outputs that called a
removed pure function remain visible with a stale indicator; unrelated
successful outputs keep their run revision. Save as persists that state, and
reopening it never restores live functions. Run the affected cells after
restoring a definition, or use **Restart and run all** for a clean replay.
The supported persistent subset remains bounded pure scalar functions and
immutable scalar values; effectful state and arbitrary imports are unavailable.

`POST /api/interrupt` replies with `{"interruptRequested":true}` when it sets the worker's interrupt signal. That reply does not claim the worker has stopped. Once the active run returns, its cell status is `failed`, retained output is a stale observation, and dependent outputs are `stale`. A later restart clears the signal before running cells again.

From the repository root, run `cargo run --offline -p cgx2-notebook-ui -- open path/to/notebook.cgnb` and open the printed `http://127.0.0.1:PORT/` address on the same computer. The UI accepts ordinary and portable `.cgnb` files. It shows saved Markdown, captured text, supported images, cell status, and provenance without executing on open. Notebook content is inserted as text in the browser; browser-side execution of notebook HTML or script is unsupported.

Use **Restart and run all** for a clean file-order evaluation. **Apply edit** changes a code cell in memory and marks it and transitive readers stale. **Run cell** executes that cell and those readers while preserving independent cells' outputs and execution revisions. **Run next** executes the next code cell in file order. Save as writes an ordinary `.cgnb` beside the input; Export portable writes a self-contained `.cgnb` with verified referenced assets. Existing destinations require the **Overwrite existing file** checkbox. Reopening reproduces saved display and provenance as cached or stale observation without executing cells or restoring live bindings.

The kernel accepts immutable scalar bindings and pure expressions only. Captured text has MIME exactly `text/plain` and a 65,536 UTF-8 byte limit per output. The notebook file limit is 16 MiB; assets are limited to 8 MiB each, 32 distinct items, and 12 MiB total. Kernel limits are 128 code cells, 64 KiB cumulative code source, and 1 MiB scalar evaluation state. External files, network effects, shell commands, Python, rich HTML execution, and Jupyter plugin parity are outside this workflow.

The server binds only `127.0.0.1`, checks loopback Host and Origin, requires a random process token for changes, enforces a 128 KiB HTTP body limit and five-second I/O deadline, and serves crate-local JavaScript and CSS under a restrictive Content Security Policy. Saved image bytes are verified by digest and served through a token-protected local route; HTML and SVG assets are rejected.

**Restart** discards live bindings and displays saved outputs as stale; **Restart and run all** immediately executes file order. **Interrupt** sends the token-protected `/api/interrupt` request while a cell is running. The 2-second per-cell scalar worker deadline also stops stalled evaluation. The server handles that signal while execution holds the session lock; the server polls worker completion and interruption every 5 ms; OS scheduling and process termination can add latency. A stopped cell is failed, dependent outputs are stale, and previously completed revisions remain available. Save persists output/provenance status, not live bindings. The in-process parse/check phase is source-size bounded but cannot be preempted by this interrupt. External side-effecting cells are unsupported, so this mechanism does not promise rollback of files or network actions.
