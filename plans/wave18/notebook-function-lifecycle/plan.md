# Notebook pure-function lifecycle

**Goal.** Make `.cgnb` pure-function experiments behave predictably when users edit, rerun, delete, restart, or interrupt cells. Keep the existing explicit rejection of effectful code.

**Ownership.** Edit only `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, and `crates/cgx2-notebook-ui/**`. Do not edit parser/checker/compiler crates, root Cargo files, shared docs, or another lane.

## Deliverable

1. Preserve a bounded pure function defined in one cell and called transitively from later cells through a save/reopen `.cgnb` round trip. Track which output used which function revision. Editing, removing, or renaming the definition must mark dependent outputs stale while leaving unrelated successful outputs intact. Reject duplicate or recursive definitions and effectful bodies with a cell-local diagnostic rather than running partial code.
2. Verify selective rerun refreshes only affected cells and that restart clears kernel state until definitions are replayed. An interrupt or failed cell must never publish a new function definition or make stale output appear fresh. Preserve notebook-format compatibility and existing output provenance.
3. Cover the same scenarios at kernel, CLI file runner, and loopback UI boundaries, including a user-visible stale indicator and actionable diagnostic. Keep execution bounded and do not introduce Python/Jupyter runtime dependence or external side effects.
4. Update owned README/examples with a short `.cgnb` demonstration and precise current limits: pure scalar functions only, no effectful persistent notebook state or arbitrary imported libraries yet.

## Verification and handoff

Run targeted offline tests for all five notebook crates, loopback UI tests where socket permission is available, formatter, and Clippy on owned packages. Reopen a committed `.cgnb` fixture through the CLI. Commit only owned paths. Report commit SHA, files, commands/results, restart/interrupt behavior, and remaining Jupyter-scale gaps; stop for review.
