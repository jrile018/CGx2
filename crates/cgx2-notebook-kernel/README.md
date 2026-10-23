# CGx2 pure notebook kernel

`remove_cell(id)` removes a code or Markdown cell without replaying other
cells. It invalidates retained definitions from that cell and marks transitive
readers' saved outputs stale. Independent outputs retain their provenance,
including after `.cgnb` save and reopen. Rerunning a reader of a removed
function reports `CGNK011` with a definition or replay hint.

`NotebookKernel` executes code cells in a process-local scalar session. Cells may declare local records and pure functions alongside immutable `let` bindings and pure expressions. Functions use ordinary `.cg` syntax: `fn name(x: i64) -> i64:` with optional empty `effects()` and `caps()`. Parameters and explicit return types accept `i64`, `Bool`, `String`, and visible local nominal records with one to eight `i64`/`Bool` fields. A record must have been declared in an earlier current cell or in the function's own cell. Identity, field projection, and constructing/returning records preserve exact scalar values, declaration-order display, source-order argument evaluation, and checked arithmetic.

Bodies admit immutable `let`, expressions, and `return` with a value. Calls to earlier or same-cell pure functions are allowed; cross-cell `let` capture, named function arguments, recursion, `main`, argument builtins, module declarations, mutable state, loops, contracts, foreign/unsafe declarations, nonempty effect/capability declarations, and unknown calls are rejected before execution. An unknown call gets `CGNK011`. Nested, generic, managed, and foreign-layout record signatures remain unsupported. Imported public scalar records are supported within the checked local package profile below.

Record annotations are static dependencies even when a parameter is unused. The dependency facts use `type:RecordName` keys for annotations and record declarations so value bindings with the same name cannot hide the schema producer. Editing, replacing, or deleting a record invalidates its functions and transitive callers; same-cell type dependencies do not create a self-edge. Definitions and pending values are committed only after the whole cell succeeds. Record results reuse the bounded tagged worker protocol and retained-schema validation. See [the record-function example](../cgx2-notebook-cli/examples/record-functions.cgnb).

`NotebookKernel::open_with_project(notebook, Some(root))` enables `import package.module` in an import-only code cell, followed by `module.function(...)` calls. The root must contain `cgx2.toml`; `cgx2-package` checks its listed, acyclic local `.cg` modules and freezes their exact source and manifest hashes. Only public functions with empty declared effects or structurally checked inferred pure effects, `i64`/`Bool` or public scalar-record parameter and return types, pure bounded bodies, and no foreign binding, contract, or capability are callable. Public records have one to eight immutable `i64`/`Bool` fields. The kernel rechecks the source snapshot before each cell, applies the existing step/depth/byte budgets, and runs calls through the scalar interpreter and killable worker. A package check validates source but does not imply the call executed successfully. Module short names must be unique; Package functions require their own explicit imports. Notebook source requires direct imports for named module access; internal dependencies load through the declared closure. The saved output provenance records the manifest SHA-256 and each used module's source SHA-256. Reopening with changed source marks dependent outputs stale; a missing project fails with `CGNK012`.

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

## Public imported scalar records (Wave 26)

[The public-record notebook and local package](../cgx2-notebook-cli/examples/public-records/public-records.cgnb)
show `alpha.Point(active: true, x: 41)`, `alpha.bump(p)`, `p.x`, and local
`fn next(p: alpha.Point) -> alpha.Point` definitions. Full record constructor/type
spelling (`demo.alpha.Point`) also requires the explicit `import demo.alpha`.
Function calls retain the existing `alpha.function` spelling. Imports occupy
separate cells and every called module must be imported directly. Declared transitive dependencies load internally. Reexports, private types, foreign code, effects and contracts remain unsupported.
A visible module root cannot also name a notebook binding, record, function,
parameter or function-local binding. Missing/private annotations and constructors
fail at the named cell and source span before executing or replaying it. Lowering
keeps inverse byte offsets so runtime/type errors refer to the original cell after
qualified names expand.

Retained record values use `defining.module.Record` identity. Each lowered
function signature/constructor has a reversible, UTF-8 byte-length-encoded
`__cgx2_pkg` symbol incorporating every module segment and the record name.
The worker carries that complete encoded identity in the existing bounded `R`
frame; qualified textual identities are accepted too, but the result must match
the independently checked expression result type and the exact checked schema.
Only an identity in the checked package is normalized. Local-only notebooks keep
their original short identities, including generated-looking local names. In a
project notebook/module the generated prefix is reserved. Fields retain declaration
order and arguments evaluate in source order; exact i64/Bool values survive replay.
The identifier/frame, field-count, type and retained-value limits remain enforced.

Import, constructor, projection, annotation-only and inactive-branch dependencies
participate in invalidation. Captures record actual manifest and module source
SHA-256 values plus an optional `root_sha256` binding the canonical package root.
The root digest hashes `CGX2-PACKAGE-ROOT-v1:` followed by lowercase hexadecimal
platform path bytes. Older captures remain readable; package captures without a
root digest are stale, while ordinary local-record captures remain compatible.
No live values are restored from saved display text. Package snapshots are checked
before execution, after worker execution, and before definition/value publication.
Detected module edits/deletes/path replacements stale their users and discard those
live definitions; manifest failures stale all package users. Independent outputs
retain their revisions. Reopen against checked inputs before executing again;
the kernel does not silently reload changed package metadata.

Portable export embeds supported notebook assets only. Imported CGx2 source and
`cgx2.toml` stay external and the matching `--project ROOT` is required after
export/reopen. This profile makes no performance, foreign-memory or effect-replay
claim. See [verification evidence](WAVE26-EVIDENCE.md).


## Declared import closure (Wave 27)

[The chain and diamond notebook](../cgx2-notebook-cli/examples/import-closure/import-closure.cgnb)
imports `demo.api` and `demo.other`. Its checked package contains the chain
`api -> left -> base` and diamond `api -> left/right -> base`. Each reached module
lowers once, in the package checker's deterministic dependency order. Unused
imports, annotations, private helper bodies and inactive branches participate in
purity checking and provenance. Unrelated modules do not become dependencies.

Direct notebook imports alone control source visibility. An inferred result may
retain `demo.base.Point` and allow `p.x`; naming that type, constructing it, or
calling its defining module requires `import demo.base`. Identically shaped
`demo.other.Point` remains a different nominal type. The worker independently
checks the expected result identity and exact scalar field schema before any
binding is published.

The frozen package effect checker treats qualified calls as `Unknown` and rejects
an explicit `effects()` declaration on such wrappers. Package wrappers therefore
omit the effect annotation. The notebook checker admits these only after checking
every reached function body against the pure scalar/record profile, and checks the
assembled lowered script again. Explicit nonempty effects, capabilities,
contracts, foreign bindings and unsupported bodies remain rejected.

Graph validation is iterative and bounded by `cgx2_package::MAX_SOURCES` (128)
unique modules and path levels, with defensive missing/cyclic/topology checks.
Diamonds share the same 32-function and 1 MiB expanded source/replay budgets;
modules do not get separate budgets. Existing 32-level static call, 100,000-step,
notebook, retained-state and worker limits still apply. Expansion sizes are checked
before replacing or appending source, and import admission fails before worker
execution or retained import publication.

Package provenance uses the same closure as executable definitions. It preserves
actual module source hashes and adds optional `module_path_sha256` entries with
exactly the same module keys. Each entry hashes `CGX2-PACKAGE-MODULE-PATH-v1:` plus
lowercase hexadecimal canonical-path bytes. This distinguishes same-byte changes
to a module's canonical target after save/reopen. Older captures without these
path digests remain readable and become stale. Root/manifest and relevant
transitive source/import/visibility/schema changes invalidate retained state and
readers; unrelated observations remain valid. A changed live frozen package still
requires reopening even if its changed module is unrelated to the selected cell.
Export preserves notebook captures/assets; package sources remain external and
require `--project ROOT`. See [verification evidence](WAVE27-EVIDENCE.md).
