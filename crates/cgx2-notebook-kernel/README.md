# CGx2 bounded notebook kernel

## Retained runtime contracts (S1-08)

Notebook-defined `i64`/`Bool` functions support ordinary `requires` and `ensures`
lines before the body, including functions with verified local `i64` accumulators
and literal-bound range loops. The existing checker validates Boolean conditions,
reserves `result`, and enforces pure predicates; the existing interpreter checks
preconditions before the body and postconditions against the returned value.
`result` is visible only in postconditions. Body locals and captured cell bindings
are unavailable to contracts. String/record contract signatures remain outside the
checker profile. These are runtime checks, not formal proofs.

Calls and arguments used only in contracts contribute to dependency invalidation,
forward-reference rejection and source diagnostics, including inactive conditions.
Pure imported predicates are lowered in both pre/postconditions and carry manifest
and transitive module hashes into saved provenance. Imported function bodies keep
their existing profile: contracts, mutation, loops and external effects are still
rejected there. Contract helpers cannot introduce hidden mutation or capabilities.

Static cost includes every condition and its transitive calls on every generated
invocation, plus retained/pending value replay. The limits remain 100,000 steps,
32 functions/static call levels, eight parameters, 1 MiB evaluation/retained state,
and the default two-second worker deadline. Recursion, unresolved calls and depth
are checked even after a constant-false condition. False pre/postconditions report
`CGR015`/`CGR016`; overflow/division errors preserve interpreter diagnostics.
Static errors map to the contract's cell source span; runtime errors identify the
calling statement's cell source span in both direct and worker modes.

Failed, timed-out or interrupted cells publish no pending bindings. Existing
displayed output remains a failed/stale observation. Selected/affected rerun,
restart/run-all, save/reopen, CLI sweeps/exports and HTTP actions use this same
admission path. See the [standalone example](../cgx2-notebook-cli/examples/runtime-contracts.cgnb),
[imported-predicate example](../cgx2-notebook-cli/examples/contract-imports/contracts.cgnb)
and [delivery evidence](S1-08-EVIDENCE.md).

Persistent external-effect notebooks remain blocked on reviewed compiler and
persistent-worker integration (S2/S3). This deliverable does not admit Python,
foreign handles, memory/collection, file/network effects or execution-once effect
cells, and does not integrate the concurrent worker crate.

## Function-local computation (Wave 28)

[The local-sums notebook](../cgx2-notebook-cli/examples/local-loops/local-loops.cgnb)
uses function-local mutable `i64` values, `=` and `+=`, nested Boolean
`if`/mandatory `else`, and ascending half-open `for i in range(start, end)`.
Both endpoints must be signed in-range i64 literals; `start >= end` is empty.
The index is immutable and scoped. Locals initialize on every invocation and
follow lexical shadowing; block locals cannot escape. Parameters, captured
cell bindings, records/fields, imported names and other scalar types cannot
be mutated. Top-level cells still use immutable bindings and expressions.

The shared checker retains inferred or explicit `effects(Mutate)`. Admission
independently proves the complete local computation body before publication;
extra effects, capabilities, foreign/memory operations and recursion
remain rejected. A dishonest `effects()` annotation still fails. Calls outside
loops may reach other verified local computational functions. Inside loops the
frozen checker requires pure helpers, including through wrappers and empty
loops. Its conservative rule also rejects record constructors in loop bodies;
existing record parameters/projections remain available.

All assignment RHS expressions, nested statements and annotations contribute
dependencies, including inactive branches and zero-trip loops. Literal range
syntax itself is not a function dependency. Imported package bodies retain the
Wave 27 profile: no new mutable/loop bodies or nonempty effect declarations.
Public pure scalar helpers may be called inside local loops after lowering.

`src/cost.rs` bounds execution before any worker starts or definitions/values
are published. It counts statements, expressions, range setup, iterations,
both branches and transitive calls. Wide literal differences and saturating
products prevent MIN/MAX and nested ranges from wrapping. Memoized call costs
retain depth and still inspect unreachable bodies for forbidden/unknown calls.
The 100,000 static-step bound includes each generated invocation, actual
retained-value replay and a conservative 42-step bound per pending binding per
later invocation (a record with eight MIN i64 fields). Generated result
retrieval costs two steps. Limits remain 32 functions/static call levels,
1 MiB cumulative evaluation/state, bounded frames and a default two-second
worker deadline. No source unrolling or alternate evaluator is used.

Selected/affected rerun, Restart and Run All, clean sweeps, save/reopen and
portable export recompute each local from its initializer. Failures/overflow,
bad worker results and cancellation never publish partial cell bindings.
Reopening restores observations; rerun prerequisites or Restart and Run All
before calling saved definitions. External effect replay remains unsupported.
See [verification evidence](WAVE28-EVIDENCE.md).

`remove_cell(id)` removes a code or Markdown cell without replaying other
cells. It invalidates retained definitions from that cell and marks transitive
readers' saved outputs stale. Independent outputs retain their provenance,
including after `.cgnb` save and reopen. Rerunning a reader of a removed
function reports `CGNK011` with a definition or replay hint.

`NotebookKernel` executes code cells in a process-local scalar session. Cells may declare local records and pure functions alongside immutable `let` bindings and pure expressions. Functions use ordinary `.cg` syntax: `fn name(x: i64) -> i64:` with optional empty `effects()` and `caps()`. Parameters and explicit return types accept `i64`, `Bool`, `String`, and visible local nominal records with one to eight `i64`/`Bool` fields. A record must have been declared in an earlier current cell or in the function's own cell. Identity, field projection, and constructing/returning records preserve exact scalar values, declaration-order display, source-order argument evaluation, and checked arithmetic.

Bodies admit immutable `let`, expressions, and `return` with a value, plus the local computation profile above. Calls to earlier or same-cell verified functions are allowed; cross-cell `let` capture, named function arguments, recursion, `main`, argument builtins, module declarations, external mutable state, dynamic loops, foreign/unsafe declarations, additional effects/capabilities, and unknown calls are rejected before execution. An unknown call gets `CGNK011`. Nested, generic, managed, and foreign-layout record signatures remain unsupported. Imported public scalar records are supported within the checked local package profile below.

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
Top-level conditionals, captured cell values in function bodies, externally effectful
functions, foreign calls and contract-bearing imported profiles retain their rejection
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
