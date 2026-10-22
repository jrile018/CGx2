# Notebook effect boundary

**Goal.** Wave 15 supports restart/interrupt for a bounded retained pure-cell profile. Make effects and stale-output propagation explicit for a small admitted source profile without presenting it as general Jupyter compatibility.

**Ownership.** Only `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, and `crates/cgx2-notebook-ui/**`. Use frozen compiler/CLI APIs; no root Cargo, shared documents, or other-lane edits.

## Deliverable

1. Define which current `.cg` cell effects can actually execute in the notebook. For those effects, record execution provenance and conservatively invalidate later dependent outputs on edit, rerun, error, timeout, interrupt, and restart. Never replay an already completed effectful cell during ordinary selective rerun. Unsupported file/network/foreign effects must get a clear diagnostic, not an apparent success.
2. Verify the worker's bounded stdin, output, timeout, and kill behavior after the Wave 15 fixes, including a nonreading child and simultaneous slow UI client/interrupt request. Track exactly when the session changes from running to interrupted and when outputs become stale. Reopening saved `.cgnb` must remain passive.
3. Add an end-to-end notebook fixture with code and Markdown cells, a changed definition, an interrupted effect-capable cell if supported, restart and Run All, saved output reopen, and status visible through CLI/UI. If no effectful profile is safely executable with frozen APIs, implement explicit rejection and document the blocker instead of fabricating persistence.

## Verification and handoff

Run offline tests for owned crates, CLI round-trip, and UI loopback tests when socket permission is available; otherwise report the precise skip. Include a state-transition table and commit ID. Stop for review.
