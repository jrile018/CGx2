# CGx2 loopback notebook UI

Retained scalar `requires`/`ensures`, including checked local accumulators, use the
same [kernel contract profile](../cgx2-notebook-kernel/README.md) as the CLI.
Open [runtime-contracts.cgnb](../cgx2-notebook-cli/examples/runtime-contracts.cgnb),
then Restart and Run All. Editing a contract-only helper stales its definitions
and callers. Failed checks retain prior displayed observations with failed
provenance; no pending bindings are published. Save/reopen and the normal worker
deadline apply. These runtime checks do not enable external-effect cells.

Open [the local-sums example](../cgx2-notebook-cli/examples/local-loops/local-loops.cgnb)
with `--project crates/cgx2-notebook-cli/examples/local-loops`. Notebook-defined
functions can use mutable i64 accumulators, signed literal-bound ranges and
nested conditionals/loops. The compiler retains `Mutate`; the kernel proves
function-local computation before admitting replay. Imported helpers inside
loops must remain pure; captured, parameter/index/record and external mutation
remain rejected. The 100,000 static-step bound includes nested calls and value
replay before worker execution. Run cell, Restart and run all, Save as and
Export portable retain their existing behavior; locals initialize per call,
and saved displays never restore live accumulators. Matching package inputs
remain required after export/reopen. See the kernel's [profile and limits](../cgx2-notebook-kernel/README.md).

Open a notebook with checked local scalar imports using
`cgx2-notebook-ui open FILE.cgnb --project ROOT [--port PORT]`. The project
must contain `cgx2.toml`; only listed, checked pure scalar modules are
available. Calls use `module.function(...)` after an import-only cell. The UI
shows the checked manifest and module source hashes in output provenance and
marks dependent outputs stale when a module changes. A missing project fails
clearly at open. External side effects, arbitrary Python libraries, direct
tensor handles, and broad Jupyter kernel compatibility remain unsupported.

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


The checked `--project ROOT` profile also supports public immutable scalar records
from explicitly/directly imported local modules. The loopback UI preserves full
module identities through selected rerun, affected rerun, saved output, Restart and
Run All, and portable export. Its state includes `package.rootSha256` alongside
manifest/module hashes. Detected stale package inputs discard affected live values;
independent cell outputs retain their revisions. Reopen the checked project before
executing after a package edit. Portable export embeds notebook assets, not the
imported `.cg` source/manifest: reopen with the same required project. See the
[public-record example](../cgx2-notebook-cli/examples/public-records/public-records.cgnb)
and [kernel boundaries](../cgx2-notebook-kernel/README.md).
