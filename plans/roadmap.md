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
| Wave 5, integrated on review branch | Execute checked memory scripts through runtime, emit checked scalar Rust, expose a notebook file runner, validate package manifests/module files, and build a conservative execution policy prototype | Combined Rust/benchmark tests, Clippy, memory/report/package/notebook smoke and six-pair parity pass; one formatting repair applied during integration. |
| Wave 6, integrated on review branch | Source owned/managed conversions, 128-seed deterministic memory corpus, native scalar compiler, loopback pure-scalar notebook UI, conservative call/effect index | Combined Rust/bench tests, format, Clippy, bridge tests, parity and component smoke passed; UI loopback tests required socket permission. |
| Wave 7, integrated on review branch | 256-seed conversion-aware memory corpus, static acyclic local imports and checked public calls, broader Python values/keywords, finite-domain verification command, native scalar pilot and LLM matrix audit | Combined Rust/Python tests, fmt, Clippy, UI HTTP tests, parity, package/verifier/memory examples and native six-case parity passed; real PyTorch/live solver absent. |
| Wave 8, dispatched from reviewed Wave 7 snapshot | Native scalar package execution, runtime state-machine fuzz, pure-scalar notebook selective rerun, measured CPU Auto policy, advisory security policy/taint output | Each bounded lane has disjoint files and explicit acceptance; no general GPU or whole-language safety claim. |
| Wave 9 and later, planned from measured gaps | Source effects/capabilities/contracts/proofs, Python/PyTorch `.cg` interop, memory audit, typed IR, native managed-memory codegen, persistent notebook side effects, safe parallel/GPU backends, and LLM/runtime holdouts | All first-release features and memory correctness gates pass; performance and LLM gains meet preregistered targets with uncertainty reported. |

Stages are dependency gates, not an automatic schedule. The coordinator may split a later stage into multiple waves if interfaces are not stable enough for safe parallel work. Review and integration time is budgeted at roughly 15–25% of each coding wave; adding a chat must save more time than the extra contract and merge work it creates.

## Coordinator loop when the user reports a wave complete

1. Identify exact commits and changed paths; compare each output with its plan and the language design. Test on the same revision and disclose skipped external gates.
2. Resolve ownership overlaps or contract drift on an integration branch, never by silently changing worker branches. Keep `main` unchanged until the user reviews the integrated result.
3. Run a small cross-component acceptance program and update the release-gap ledger. State precisely which features are implemented, simulated, or still absent.
4. Create a bounded next-wave ledger and one detailed `plan.md` per independent coding chat. Start every chat from the same verified integration commit; no chat edits another lane's paths.
5. Dispatch the chats only for work whose input contract is already stable. Each chat commits and stops. Wait for the user to say the wave is complete before starting the next.

## Current gap ledger

Wave 7 added local import graph/type checks, conversion-aware 256-seed memory conformance, bounded Python keyword/structured values, an external-contract verifier command, and native scalar/LLM benchmark readiness tools. Combined non-UI Rust tests, all five UI HTTP tests with loopback permission, Python bridge tests, format, Clippy, 35 benchmark unittests, six-case Python parity, six-case native Rust parity/144 launch samples, and component smoke passed. Real PyTorch is absent; no live SMT solver or genuine LLM task traces exist. Imported code cannot execute yet, source effects/capabilities/contracts remain unenforced, managed memory lacks native codegen, notebook side effects remain unsupported, and general GPU execution is absent. Wave 8 uses five disjoint stable-API lanes to connect packages to a native executable and advance memory, notebook, CPU policy, and security evidence. The hard memory correctness gate remains open despite finite corpora.
