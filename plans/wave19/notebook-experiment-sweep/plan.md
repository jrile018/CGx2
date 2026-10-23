# Bounded native notebook experiment sweeps

**Goal.** Let a user compare small pure `.cg` experiments in the native `.cgnb` workflow without manually duplicating notebooks or sharing mutable kernel state between runs.

**Ownership.** Edit only `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, and `crates/cgx2-notebook-ui/**`. Do not edit language/parser/checker/compiler crates, root Cargo files, or shared docs.

## Deliverable

1. Add a bounded sweep over a selected parameter cell whose entire code source is one immutable scalar literal binding, such as `let factor = 2`. Accept values of the same checked scalar type (`i64`, `Bool`, or `String`), at most 32 values. For each value, start a clean kernel, replace only the selected cell source, run cells in file order, and write a distinct `.cgnb` output with current/stale/failed provenance and a machine-readable summary. Do not reuse live bindings or outputs across trials.
2. Expose a CLI command with explicit input, cell ID, values, and output directory. Reject invalid IDs, type changes, duplicate values if they make output ambiguous, path traversal, symlinks, existing output collisions, output outside destination, oversized values, and non-pure/effectful cells before publishing results. Use staged writes or equivalent so a failed validation never leaves a partial published sweep. Per-cell timeout and existing notebook size limits still apply.
3. Provide a small UI path to view or launch the same bounded sweep only if the existing loopback security/CSRF model can be preserved. Otherwise expose the CLI and add UI documentation to open the resulting `.cgnb` files; do not add an unauthenticated execution endpoint. Add a committed example and tests covering mixed success/failure values, independent outputs, save/reopen, restart, and no file/network effects from cell code.
4. Keep `.cgnb` version 1 compatibility unless a strictly additive, documented format change is necessary. Report that the feature explores pure scalar variations; it is not a Jupyter kernel or general side-effect replay system.

## Verification and handoff

Run targeted offline tests for all five notebook crates, loopback UI tests if socket permission is available, formatter, and Clippy. Demonstrate a sweep and inspect two reopened outputs. Commit only owned paths. Report SHA, files, exact commands/results, timeout/partial-run behavior, and current notebook limits; stop for review.
