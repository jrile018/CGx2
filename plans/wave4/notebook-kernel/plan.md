# CGx2 Pure-Scalar Notebook Kernel Plan

> Implement this plan in your attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Execute `.cgnb` code cells with CGx2's checked scalar evaluator and make successive pure cells share visible bindings through safe replay. Deliver an honest kernel library, not a graphical notebook claim.

**Architecture:** New `cgx2-notebook-kernel` crate consumes `cgx2-notebook-format`, `cgx2-notebook-session`, `cgx2-syntax`, and `cgx2-cli` public APIs from Wave 3. A notebook run starts in a clean session; each accepted pure code cell is appended to a bounded transcript and the transcript is checked/evaluated from the beginning. The last cell's tail value becomes its `text/plain` output. Replaying prior cells is valid only in the documented pure scalar subset. The notebook session handles output ordinals and dependency invalidation.

**Tech stack:** Rust 1.98, existing workspace crates, offline dependencies only. No browser or UI framework this wave.

**Specification:** `docs/language-design.md` notebook sections, `crates/cgx2-notebook-format/README.md`, `crates/cgx2-notebook-session/README.md`, and `plans/wave4/README.md`.

## Owned paths and boundaries

- Own only new `crates/cgx2-notebook-kernel/**` and this plan. Root wildcard membership discovers the crate. Do not edit CLI, syntax, sema, format/session, runtime, or root manifests.
- The frozen scalar `cgx2_cli::check_source` and `cgx2_cli::run_source` are the execution APIs. Parse source with `cgx2_syntax::parse_module` to extract simple `defines`/`uses` facts. Treat unresolved qualified names, function definitions, loops, memory intrinsics, and future/unknown AST forms as unsupported until replay semantics are proved.
- Keep the kernel deterministic and resource bounded: define max cells, cumulative source bytes, and evaluation steps or timeout behavior. The CLI evaluator already has a step limit; the kernel should bound cumulative replay size and cell count.
- Never emit an output for a failed cell. A failed execution must not append that source to the accepted transcript. Restart clears accepted source and runs cells in order. Direct rerun of an already executed cell either rebuilds from a clean prefix safely or returns a documented `RequiresRestart` error; do not duplicate `let` bindings silently.

## Review focus

1. `let x = 3` in one cell and `x + 4` in the next produces `7` as text output, using CGx2 checking and arithmetic.
2. An invalid later cell does not poison a previously accepted prefix; changing an earlier definition marks dependents stale.
3. `restart_and_run_all` is deterministic and clears old outputs when a run fails. Markdown cells never execute.
4. Duplicate definitions, unsupported effects/functions/loops/memory, overlong notebooks, and direct rerun policy are explicit errors rather than accidental replay.
5. `.cgnb` serialization after execution reopens with the expected cell IDs, sources, ordinals, and text MIME outputs.

## Tasks

### 1. Public kernel API and facts

- [ ] Add red tests for sequential pure cells, unknown names, escaped strings, and fact extraction. Define `NotebookKernel::open`/`run_cell`/`restart_and_run_all` (or equivalent) using the existing `NotebookSession` API.
- [ ] Implement a `CellExecutor` that stores only accepted pure cell source and rechecks/replays a bounded concatenated script. Build `CellFacts` from AST names; if precise analysis is not possible, conservatively set `opaque_effects` or reject.
- [ ] Document accepted expression/statement subset and direct rerun policy.

### 2. File and failure behavior

- [ ] Add fixture `.cgnb` with Markdown and two code cells, plus tests for roundtrip output. Preserve Markdown and never run it.
- [ ] Add tests for failure atomicity, duplicate binding, edit/restart, size limits, and any unsupported code form. If the public session API lacks an operation needed for safe edit/restart, implement an adapter within this crate rather than editing another lane's crate.
- [ ] Ensure stable error codes/messages suitable for a future CLI and UI. Do not invent an HTML/JavaScript UI in this lane.

### 3. Verification and handoff

- [ ] Run `cargo test -p cgx2-notebook-kernel --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-notebook-kernel --all-targets --offline -- -D warnings`.
- [ ] Update the crate README with a short runnable API example and limitations: pure scalar only, replay cost, no live graphical UI, no Python/PyTorch or memory operations. Commit only owned paths.

**Acceptance:** A real `.cgnb` file can be checked and executed in a clean pure-scalar session through a library with deterministic outputs and safe rerun limitations. A user-facing notebook UI remains a later milestone.
