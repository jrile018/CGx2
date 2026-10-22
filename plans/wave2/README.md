# Wave 2: first executable preview and memory path

All four chats start from the same `wave2/integration` commit after the coordinator includes Wave 1 and the root Cargo workspace. The shared parser AST from Wave 1 is **frozen for this wave**. This is a tactical, one-human/multiple-chat split: each worker makes one bounded commit and stops for review. Estimated coordination cost is 15–25%, including this contract, four reviews, and integration.

| Track | Exclusive write surface | Consumes | Produces |
| --- | --- | --- | --- |
| [Semantic checker](sema/plan.md) | `crates/cgx2-sema/**`, `plans/wave2/sema/plan.md` | Wave 1 syntax AST | Static name/type/mutability/return diagnostics and typed summary |
| [CLI execution preview](cli/plan.md) | `crates/cgx2-cli/**`, `plans/wave2/cli/plan.md` | Wave 1 syntax AST | `cgx2 run FILE.cg -- ARGS`, shortcut, formatter, checked preview interpreter |
| [Owned memory](memory/plan.md) | `crates/cgx2-runtime/**`, `plans/wave2/memory/plan.md` | Wave 1 `Session`, budgets and handles | Unique owned byte buffers and explicit managed transitions |
| [Notebook session](notebook/plan.md) | `crates/cgx2-notebook-session/**`, `plans/wave2/notebook/plan.md` | Wave 1 `.cgnb` model | Persistent cell session, dependency invalidation, restart/run all |

## Index-card contracts and deliberate seams

- The syntax crate is read-only this wave. The semantic checker and CLI both consume the exact Wave 1 `cgx2_syntax::Module` and `Span` types. Neither may add syntax or modify the parser. This keeps their source dependency stable while they run in parallel.
- The CLI **executes** the existing subset and returns explicit runtime or unsupported errors. It does not claim `cgx2 check` or static type safety. The semantic checker **analyzes** that subset and does not execute it. The coordinator joins them in Wave 3; no worker invents an adapter across the other worker's branch.
- The memory worker keeps `Session`, `Handle`, `RegionId`, `CollectionMode`, and existing tests compatible. It adds unique owned buffers through the runtime crate; no other worker edits the runtime.
- The notebook worker uses `cgx2_notebook_format::{Notebook, Cell, Output}` and a trait for execution; it cannot depend on the still-changing CLI. The coordinator supplies the real execution adapter after this wave.
- All new crates join the root `members = ["crates/*"]` workspace automatically. Workers do not edit root `Cargo.toml`, root `Cargo.lock`, shared docs, `AGENTS.md`, or other plans. They may build offline and report if an uncached dependency would be required.

## Handoff and review

Each worker runs its plan's exact tests, format check, and Clippy; commits only owned paths; reports commit, tests, API, and limits; then stops. The user reviews each result. The coordinator combines reviewed commits on an integration branch, checks the full workspace and a CLI smoke program, and asks for no next-wave work until the user reports these chats complete. No worker merges to `main` or launches another chat.

Potential integration conflict is semantic, not file-level: the preview interpreter and static checker may disagree on builtin calls or tail-expression behavior. Both plans pin those rules; the coordinator will compare fixtures before their outputs are treated as one language implementation. A fifth chat for source emitters or parser expansion would depend on the typed IR or change the shared AST, so it is held for the next gate.
