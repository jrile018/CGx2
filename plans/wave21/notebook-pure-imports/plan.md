# Read-only pure local package imports in `.cgnb`

**Goal.** Let a notebook use checked local CGx2 modules for small experiments while retaining clean, reproducible cell runs and the existing effect boundary.

**Ownership.** Edit only the five `crates/cgx2-notebook-*` crates and their owned fixtures/examples. Consume the committed Wave 20 `cgx2-package` interface without editing that crate, source compiler, root Cargo files, or shared docs. If a missing package API blocks execution, document the exact boundary for coordinator review rather than changing another lane.

## Deliverable

1. Add one explicit project-root option to notebook CLI/UI startup (for example `--project ROOT`) and a notebook code-cell `import package.module` form. Resolve only acyclic local modules listed by `cgx2.toml`; no path traversal, network fetch, ambient Python import, or unpinned dependency search. Keep notebook syntax in `.cgnb`, not an `.ipynb` wrapper. Without the option, existing standalone pure cells behave exactly as before.
2. Admit only declared pure scalar public functions with supported `i64`/`Bool` signatures from the checked package. Call spelling is `module.function(...)`, with the module segment bound by a preceding `import package.module`; duplicate module short names fail explicitly, and transitive calls outside directly imported modules are rejected in this slice. The package checker supplies validation and metadata, not execution: implement an owned notebook-kernel lowering or dispatch path that executes those checked local functions through the existing interpreter entry point, preserving source spans, source-order evaluation, step/depth budgets, and cancellation. Do not treat a successful package check as proof that a function can execute. Reject foreign, allocation, collection, file/network, and capability-bearing functions before any cell runs. Include source-located diagnostics naming the rejected import/call. Record package source hashes and manifest identity in output provenance; a changed module invalidates dependent outputs on reopen/rerun.
3. Execute distinct `module.function(...)` calls from two local pure modules in one notebook; edit an earlier cell, selectively rerun dependents, save, reopen, and run both calls cleanly. Reopening with a changed or missing project must mark results stale or fail clearly; it must not present old output as current. Preserve portable markdown images and Wave 20 exact sidecar save behavior. Notebook worker subprocess timeouts and cancellation still apply.
4. Test malformed manifest/import cycles, nonpublic symbol, effectful declaration, changed source hash, missing project, output provenance, and CLI/UI parity. State that persistent external side effects, arbitrary Python libraries, direct tensor handles, and broad Jupyter kernel compatibility remain outside this slice.

## Verification and handoff

Run offline tests for all five notebook crates, including loopback UI tests where socket binding is allowed, formatter, and owned Clippy `-D warnings`. Commit only owned paths; report SHA, example notebook/project, exact commands/results, security boundary, and unsupported profile. Stop for review.
