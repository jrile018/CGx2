# Notebook restart and interruption control

Use the attached worktree; read shared Wave 15 references and this plan. Commit only owned paths and stop for review.

**Goal.** A Jupyter-like `.cgnb` workflow needs predictable restart and interruption when a cell stalls or fails. Preserve Wave 14 retained state and captured output without replaying completed cells during ordinary selective rerun. External file/network side effects remain outside this bounded lane.

**Ownership.** Only `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, and `crates/cgx2-notebook-ui/**`. Consume frozen CLI/parser APIs; do not edit them, root Cargo files, docs, plans or other lanes.

## Deliverable

1. Add a documented `restart` operation that discards live execution state and marks saved outputs as stale cache with provenance intact. A subsequent run executes code cells in file order from a clean session; reopening a saved notebook never executes cells implicitly.
2. Add bounded execution cancellation/interrupt for kernel, CLI and loopback UI. The stopped cell must not be marked current, dependent outputs become stale, earlier completed cell revisions and outputs remain intact, and a later run cannot inherit half-mutated state. If in-process evaluation cannot be preempted safely, use a separate worker/process or cooperative safe points with a documented hard bound; do not claim an unimplemented hard interrupt.
3. Test long-running/budget-exhausted cell, successful cell before interruption, dependent cell after interruption, restart/reopen, stale display and later successful rerun. Keep output MIME `text/plain`, size bounds, stable IDs and sanitized UI rendering. Expose user-facing commands and visible status without changing ordinary `.cg` syntax.
4. Update owned README with exact timeout/cancellation semantics, what is persisted, and why external side-effecting cells remain unsupported.

## Verification

Run offline tests for the five owned crates, format and targeted Clippy with warnings denied, CLI round-trip and loopback UI tests where socket permission is available. Report observed interruption bound, preserved revisions/output, remaining effect limitation and commit SHA.

**Acceptance.** Restart and interrupt have deterministic, tested state/output semantics; an interrupted cell cannot appear successfully executed.
