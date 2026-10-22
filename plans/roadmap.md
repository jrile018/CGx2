# CGx2 iterative parallel delivery roadmap

## Goal and completion standard

The release goal is the complete **first usable release contract** in `docs/language-design.md`, with the evidence gates in `docs/validation-plan.md`. It includes safe managed and owned memory, selectable collection regions and `no_gc`, modules, types/effects/capabilities/contracts, a runnable CLI, Python/PyTorch, native notebooks, Rust and Python source output for their stated profiles, bounded proof, security/performance analysis, and measured LLM and runtime value. OCaml-scale breadth is a longer-term product goal and cannot honestly be declared complete from a fixed number of waves.

Do not call a component or preview a release merely because its standalone tests pass. Each wave ends with committed, reviewable worktrees. The user reports completion and reviews the outputs; the coordinator then checks them, integrates on a separate branch, runs cross-component checks, and defines the next wave. No automation, scheduled continuation, worker-to-worker messaging, or automatic merge to `main` is part of this loop.

## Current wave and later dependency gates

| Stage | Objective | Gate before the next stage |
| --- | --- | --- |
| Wave 1, completed | Standalone syntax, managed byte-object runtime, notebook file format, Python bridge, benchmark harness, bounded verifier | Six component suites pass; limits disclosed. |
| Wave 2, completed | First `.cg` scalar execution preview, baseline static semantics, owned-buffer runtime transitions, notebook session state | Four outputs integrated on the review branch; standalone and combined suites pass. |
| Wave 3, integrated on review branch | CLI static gate and JSON diagnostics, ownership operation checker, checked Python scalar emitter, public API index | Combined Rust/benchmark suites, format, Clippy, and scalar CLI smoke pass. |
| Wave 4, integrated on review branch | Memory syntax/source checker, runtime memory reports, pure-scalar notebook kernel, typed PyTorch bridge expansion, executable six-pair pilot | Combined Rust/benchmark suites, fmt, Clippy, six-pair parity and timing pilot passed; real PyTorch absent. |
| Wave 5, dispatched from reviewed Wave 4 snapshot | Execute checked memory scripts through runtime, emit checked scalar Rust, expose a notebook file runner, validate package manifests/module files, and build a conservative execution policy prototype | End-to-end memory tests and cross-target scalar fixtures pass; other lanes remain explicit bounded foundations. |
| Wave 6 and later, planned from measured gaps | Broaden memory profiles and module imports, effects/contracts/proofs, notebook UI and persistent side-effect handling, Python/PyTorch source interop, native backend, safe parallel/GPU execution, security analysis, fuzzing, and LLM/runtime holdouts | All first-release features and memory correctness gates pass; performance and LLM gains meet preregistered targets with uncertainty reported. |

Stages are dependency gates, not an automatic schedule. The coordinator may split a later stage into multiple waves if interfaces are not stable enough for safe parallel work. Review and integration time is budgeted at roughly 15–25% of each coding wave; adding a chat must save more time than the extra contract and merge work it creates.

## Coordinator loop when the user reports a wave complete

1. Identify exact commits and changed paths; compare each output with its plan and the language design. Test on the same revision and disclose skipped external gates.
2. Resolve ownership overlaps or contract drift on an integration branch, never by silently changing worker branches. Keep `main` unchanged until the user reviews the integrated result.
3. Run a small cross-component acceptance program and update the release-gap ledger. State precisely which features are implemented, simulated, or still absent.
4. Create a bounded next-wave ledger and one detailed `plan.md` per independent coding chat. Start every chat from the same verified integration commit; no chat edits another lane's paths.
5. Dispatch the chats only for work whose input contract is already stable. Each chat commits and stops. Wait for the user to say the wave is complete before starting the next.

## Current gap ledger

Wave 4 added memory syntax, conservative source-to-ownership checks, runtime reports, a pure-scalar notebook kernel, typed PyTorch math/probe, and a real six-pair scalar benchmark pilot. Combined checks pass. Memory source still cannot run; notebooks have no user-facing command or UI; real PyTorch is absent here; no native Rust source output, package imports, source contracts/effects/capabilities, live SMT proof, or LLM benchmark result exists. The timing pilot measures fresh subprocess startup-heavy work, so it cannot establish native runtime speed. Wave 5 gives CLI memory execution exclusive ownership and keeps other lanes on separate crates. Its execution policy is a correctness-first prototype, not proof that arbitrary CGx2 code automatically parallelizes or runs on GPUs.
