# CGx2 Selective Pure-Scalar Notebook Rerun Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Let a user select and rerun a supported pure-scalar `.cgnb` code cell after an edit, with correct dependent-output invalidation and explicit clean-session behavior in the local UI.

**Architecture:** Extend `NotebookKernel` with a `rerun_cell(id)` path that replays only the necessary pure scalar prefix into a new bounded evaluator state, executes the selected cell, and marks downstream readers stale using `NotebookSession` facts. Expose a `Run this cell` action in `cgx2-notebook-ui` with server-side validation; retain `Run next` and `Restart and run all`. Saved outputs remain cached observations with status and must never be mislabeled current after an edit or failure.

**Tech stack:** Rust 1.98, existing notebook session/kernel/UI crates and `.cgnb` format, local loopback HTTP tests.

**Specification:** `docs/language-design.md` notebook semantics, notebook crate READMEs, `plans/wave8/README.md`.

## Owned paths and limits

- Own only `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-ui/**`, and this plan. Do not edit format, CLI, scalar evaluator, syntax, root files, or other lanes. Preserve current `.cgnb` serialization and previously supported HTTP routes/API behavior.
- Pure scalar code remains limited to immutable `let` bindings and scalar expressions; no I/O, Python, memory scripts, shell, or other effects. Replaying earlier pure cells internally is allowed only within the documented time/byte/step budgets and cannot run prior side effects because this profile rejects them. Do not claim general Jupyter side-effecting rerun semantics.
- Editing a cell invalidates its old result and transitive readers. Rerunning an earlier cell must rebuild a coherent evaluator environment from source, update its fresh output, and keep later outputs stale until rerun or clean Run All. Repeated rerun must not duplicate definitions or retain stale values. A failed rerun must not display old output as current. Markdown and saved rich assets stay inert/preserved according to existing rules.
- The UI accepts a stable cell ID, not arbitrary source/file paths from the browser. Preserve loopback Host/Origin/token protections, request limits, text escaping, explicit save and overwrite rules. The new action must not expose a general code execution endpoint beyond notebook cells.

## Tasks

### Kernel semantics

- [x] Add red tests for rerunning middle/first/last cells, edit then rerun, transitive dependency stale states, repeated rerun, failure after a prior success, and budget exhaustion during replay.
- [x] Implement `rerun_cell` with explicit status/output semantics. Prefer a simple coherent replay over hidden incremental state mutations; maintain stable cell IDs and ordinals.
- [x] Document exactly when prefix cells are replayed and how this differs from side-effecting notebooks.

### Local UI

- [x] Add an accessible `Run this cell` control, status/error display, and HTTP endpoint. Test old routes and new route with valid/invalid cell IDs, token/Origin failures, and HTML-like content.
- [x] Browser smoke: edit cell one, rerun it, observe a later dependent output stale, rerun later cell, save, reopen valid `.cgnb`, then clean Restart and Run All.

### Verification and handoff

- [x] Run `cargo test -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-ui --offline` (HTTP tests require local loopback permission), `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-ui --all-targets --offline -- -D warnings`.
- [x] Commit only owned paths; report rerun semantics, smoke result, and unsupported side effects.

**Acceptance:** A selected pure-scalar cell can be rerun from a coherent bounded state, dependent outputs are accurately stale/current, and the local UI/save path preserves safety limits.
