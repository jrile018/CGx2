# CGx2 pure notebook kernel

`remove_cell(id)` removes a code or Markdown cell without replaying other
cells. It invalidates retained definitions from that cell and marks transitive
readers' saved outputs stale. Independent outputs retain their provenance,
including after `.cgnb` save and reopen. Rerunning a reader of a removed
function reports `CGNK011` with a definition or replay hint.

`NotebookKernel` executes code cells in a process-local scalar session. Cells may declare local records and pure functions alongside immutable `let` bindings and pure expressions. Functions use ordinary `.cg` syntax: `fn name(x: i64) -> i64:` with optional empty `effects()` and `caps()`. Parameters and explicit return types accept `i64`, `Bool`, `String`, and visible local nominal records with one to eight `i64`/`Bool` fields. A record must have been declared in an earlier current cell or in the function's own cell. Identity, field projection, and constructing/returning records preserve exact scalar values, declaration-order display, source-order argument evaluation, and checked arithmetic.

Bodies admit immutable `let`, expressions, and `return` with a value. Calls to earlier or same-cell pure functions are allowed; cross-cell `let` capture, named function arguments, recursion, `main`, argument builtins, module declarations, mutable state, loops, contracts, foreign/unsafe declarations, nonempty effect/capability declarations, and unknown calls are rejected before execution. An unknown call gets `CGNK011`. Nested, generic, managed, imported, and foreign-layout record signatures remain unsupported.

Record annotations are static dependencies even when a parameter is unused. The dependency facts use `type:RecordName` keys for annotations and record declarations so value bindings with the same name cannot hide the schema producer. Editing, replacing, or deleting a record invalidates its functions and transitive callers; same-cell type dependencies do not create a self-edge. Definitions and pending values are committed only after the whole cell succeeds. Record results reuse the bounded tagged worker protocol and retained-schema validation. See [the record-function example](../cgx2-notebook-cli/examples/record-functions.cgnb).

`NotebookKernel::open_with_project(notebook, Some(root))` enables `import package.module` in an import-only code cell, followed by `module.function(...)` calls. The root must contain `cgx2.toml`; `cgx2-package` checks its listed, acyclic local `.cg` modules and freezes their exact source and manifest hashes. Only public functions with explicit `effects()`, `i64`/`Bool` parameter and return types, scalar bodies, and no foreign binding or capability are lowered to private notebook functions. The kernel rechecks the source snapshot before each cell, applies the existing step/depth/byte budgets, and runs calls through the scalar interpreter and killable worker. A package check validates source but does not imply the call executed successfully. Module short names must be unique; package calls to another module require that module to have been imported directly. The saved output provenance records the manifest SHA-256 and each used module's source SHA-256. Reopening with changed source marks dependent outputs stale; a missing project fails with `CGNK012`.

This slice excludes persistent external side effects, arbitrary Python libraries, direct tensor handles, allocation/collection, file/network effects, and broad Jupyter kernel compatibility.

Definitions remain in the live session and are passed to the frozen `check_source` and `run_source` APIs in a function-only module with a generated `main` wrapper. Earlier cells are never rerun to rebuild definitions. `replace_cell` and failed, timed-out, or interrupted runs invalidate a function definition and its transitive callers through function-name dependencies. `rerun_affected` recomputes only affected cells; independent outputs keep their revisions. A removed definition makes a dependent call fail until the call is changed. `restart` discards all live definitions; reopening a saved `.cgnb` restores cached observations only and never executes source. Opaque effects still make their own and later saved outputs stale.

The kernel accepts at most 128 code cells and 64 KiB cumulative code source. At most 32 pure functions, 8 parameters per function, 32 static call levels, and 100,000 estimated scalar steps per cell are admitted. Evaluation scripts and retained state have a 1 MiB cumulative budget. Captured `text/plain` output is at most 65,536 UTF-8 bytes and includes schema 2 source and transitive dependency provenance. The CLI and UI use a killable worker with a default 2 second cell deadline. The frozen evaluator also enforces its own 1,000,000 step and 64 call-depth limits. In-process parsing and static checking are bounded by source size but are not preempted by the worker timeout.

`run_cell(id)` executes the next code cell in file order. `rerun_cell(id)` executes just that cell; `rerun_affected(id)` also runs stale transitive readers. Forward dependencies produce `CGNK009`; a noncurrent prerequisite produces `CGNK004`. Failed execution retains earlier displayed text as a stale observation. Rich HTML or script is never executed by the kernel.

## Bounded Boolean conditional profile (Wave 24)

Pure function bodies support nested Boolean `if`/mandatory `else` statements
on the existing scalar/local-record profile. Both branches and the guard
contribute function/type dependencies; lexical locals stay within their branch.
Changing a function used only in an inactive branch stales users. Replay admission
counts both branches conservatively; runtime limits, transactional publication,
worker timeout/cancellation, save/reopen and Restart and Run All remain in place.
Top-level conditionals, captured cell values in function bodies, mutable/effectful
functions, foreign calls and contract-bearing profiles retain their rejection
boundaries. Cell values may be passed as ordinary arguments. See
[the notebook example](../cgx2-notebook-cli/examples/conditionals.cgnb).
