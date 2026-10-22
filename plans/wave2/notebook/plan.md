# CGx2 Persistent Notebook Session Implementation Plan

> **For agentic workers:** Implement this plan task by task in your attached worktree. Do not delegate, launch another chat, or edit another lane. Track the steps below and stop after your committed deliverable.

**Goal:** Add the native notebook's persistent execution and stale-output state machine without depending on the unfinished CLI interpreter.

**Architecture:** A new `cgx2-notebook-session` library holds a `cgx2_notebook_format::Notebook`, per-cell dependency facts, cached outputs and status, plus a caller-supplied stateful executor. An executor trait permits deterministic tests now and the real CGx2 evaluator adapter after this wave.

**Tech Stack:** Rust 1.98, Cargo offline, `cgx2-notebook-format` path dependency, standard library.

**Spec:** `docs/language-design.md` native notebook section and `plans/wave2/README.md` notebook seam.

## Global Constraints

- Own only `crates/cgx2-notebook-session/**` and this plan file. Do not modify the file-format crate, CLI, parser, root manifests, or shared docs.
- The executor stays alive across individual `run_cell` calls. Running one cell must not replay earlier cells or their side effects.
- `restart_and_run_all` calls executor reset, clears cached execution status, and executes code cells in file order; Markdown is never executed.
- `CellFacts` are supplied by a future compiler adapter and contain defined symbols, read symbols, and `opaque_effects`. The session must not pretend a text-only scan has complete dependency knowledge.
- Changing a definition marks transitive dependent outputs stale. When facts include uncertain/opaque side effects, conservatively mark later code cells stale. Preserve cached outputs but expose their stale state distinctly.
- Serialization remains the responsibility of `cgx2-notebook-format`; do not invent a competing `.cgnb` syntax.

## Review Focus

1. Rerunning a later cell must not repeat an earlier side effect; test with a counting fake executor in Task 2.
2. Editing a cell's definition must stale transitive dependents but not unrelated earlier cells; test in Task 2.
3. An opaque effect must stale later code cells even without a symbol edge; test in Task 2.
4. A failed execution must not present an old cached output as current; test in Task 3.
5. Restart and Run All must start with a clean executor state and monotonically order new outputs; test in Task 3.

## Files and public contract

- Create `crates/cgx2-notebook-session/Cargo.toml`, `src/lib.rs`, `src/deps.rs`, `src/session.rs`, `tests/session.rs`, and `README.md`.
- Export `CellFacts { defines: BTreeSet<String>, reads: BTreeSet<String>, opaque_effects: bool }`, `CellStatus::{NeverRun, Current, Stale, Failed}`, `CellResult { text: Option<String> }`, and `CellExecutor` with `execute(&mut self, source: &str) -> Result<CellResult, String>` and `reset(&mut self) -> Result<(), String>`.
- Export `NotebookSession<E: CellExecutor>::new(notebook: Notebook, facts: BTreeMap<String, CellFacts>, executor: E) -> Result<Self, SessionError>`, `run_cell(&mut self, id: &str) -> Result<(), SessionError>`, `replace_cell(&mut self, id: &str, source: String, facts: CellFacts) -> Result<(), SessionError>`, `restart_and_run_all(&mut self) -> Result<(), SessionError>`, `status(&self, id: &str) -> Result<CellStatus, SessionError>`, and `notebook(&self) -> &Notebook`.
- `run_cell` converts `CellResult.text` into `Output::Text { mime: "text/plain", ordinal, body }`; it replaces that cell's cached output on success. Use checked `u64` ordinals and structured errors for missing IDs, Markdown execution, duplicate facts, and executor failure.

### Task 1: Session construction and status

- [ ] Add failing tests for valid code/Markdown cells, unknown or duplicate fact IDs, initial `NeverRun` status, and refusal to execute Markdown.
- [ ] Run `cargo test -p cgx2-notebook-session --offline` and record failures.
- [ ] Implement session construction, ID validation, status storage, and the executor trait.
- [ ] Re-run the tests and verify Task 1 cases pass.

### Task 2: Persistent execution and dependency invalidation

- [ ] Add failing tests for single-cell run, rerun replacing that cell's output, no earlier side-effect replay, transitive symbol dependencies, unrelated cells, and opaque effects.
- [ ] Run the tests to see missing behavior, then implement `run_cell`, `replace_cell`, and dependency traversal with cycle-safe visited sets.
- [ ] Re-run the tests and verify cached outputs remain visible but marked `Stale` after edits.

### Task 3: Clean restart and error path

- [ ] Add failing tests for executor failure, previous output becoming noncurrent, reset failure, fresh state on Restart and Run All, Markdown skip, and new output ordinals.
- [ ] Run the tests to see failures, then implement `restart_and_run_all` and structured `SessionError` behavior.
- [ ] Run `cargo test -p cgx2-notebook-session --offline`, `cargo fmt --check --manifest-path crates/cgx2-notebook-session/Cargo.toml`, and `cargo clippy -p cgx2-notebook-session --all-targets --offline -- -D warnings` from the repository root.
- [ ] Document trait boundaries, cached-output/stale semantics, and absence of real `.cg` execution or UI. Commit only owned paths and stop for review.

**Acceptance:** a fake executor demonstrates persistent single-cell execution, conservative stale detection, and clean restart. This library does not yet execute CGx2 source or provide the native notebook interface.
