# CGx2 iterative parallel delivery roadmap

## Goal and completion standard

The release goal is the complete **first usable release contract** in `docs/language-design.md`, with the evidence gates in `docs/validation-plan.md`. It includes safe managed and owned memory, selectable collection regions and `no_gc`, modules, types/effects/capabilities/contracts, a runnable CLI, Python/PyTorch, native notebooks, Rust and Python source output for their stated profiles, bounded proof, security/performance analysis, and measured LLM and runtime value. OCaml-scale breadth is a longer-term product goal and cannot honestly be declared complete from a fixed number of waves.

Do not call a component or preview a release merely because its standalone tests pass. Each wave ends with committed, reviewable worktrees. The user reports completion and reviews the outputs; the coordinator then checks them, integrates on a separate branch, runs cross-component checks, and defines the next wave. No automation, scheduled continuation, worker-to-worker messaging, or automatic merge to `main` is part of this loop.

## Current wave and later dependency gates

| Stage | Objective | Gate before the next stage |
| --- | --- | --- |
| Wave 1, completed | Standalone syntax, managed byte-object runtime, notebook file format, Python bridge, benchmark harness, bounded verifier | Six component suites pass; limits disclosed. |
| Wave 2, dispatched from integration snapshot | First `.cg` execution preview, baseline static semantics, owned-buffer runtime transitions, notebook session behavior | The four outputs integrate and a source file runs through CLI; static and runtime errors are distinct. |
| Wave 3, planned after Wave 2 review | Unite evaluator with typed IR; parse and check `no_gc`, regions, imports, effects/capabilities; connect notebook session to the same evaluator | Memory programs execute through CLI/notebook with zero collection in a guarded block and rejected unsafe escapes. |
| Wave 4, planned after contracts stabilize | Native Rust emission, Python subset emission, CPython/PyTorch adapter, contract-to-verifier lowering, package/module graph, CLI commands | Differential fixtures agree on supported targets; unsupported constructs fail explicitly; real PyTorch gate runs in a selected environment. |
| Wave 5 and later, planned from measured gaps | Harden diagnostics/API index, security/performance analyzers, fuzzing, docs and editor UX; run LLM/runtime benchmarks and repair failures | All first-release features and memory correctness gates pass; performance and LLM gains meet preregistered targets with uncertainty reported. |

Stages are dependency gates, not an automatic schedule. The coordinator may split a later stage into multiple waves if interfaces are not stable enough for safe parallel work. Review and integration time is budgeted at roughly 15–25% of each coding wave; adding a chat must save more time than the extra contract and merge work it creates.

## Coordinator loop when the user reports a wave complete

1. Identify exact commits and changed paths; compare each output with its plan and the language design. Test on the same revision and disclose skipped external gates.
2. Resolve ownership overlaps or contract drift on an integration branch, never by silently changing worker branches. Keep `main` unchanged until the user reviews the integrated result.
3. Run a small cross-component acceptance program and update the release-gap ledger. State precisely which features are implemented, simulated, or still absent.
4. Create a bounded next-wave ledger and one detailed `plan.md` per independent coding chat. Start every chat from the same verified integration commit; no chat edits another lane's paths.
5. Dispatch the chats only for work whose input contract is already stable. Each chat commits and stops. Wait for the user to say the wave is complete before starting the next.

## Current gap ledger

Wave 1 did **not** produce a compiler, `cgx2` executable, owned/borrow checker, notebook execution, real PyTorch test, live SMT proof, source emitter, or benchmark result against other languages. The four Wave 2 plans address the first executable preview and the highest-priority memory path; they do not claim to finish the release contract.
