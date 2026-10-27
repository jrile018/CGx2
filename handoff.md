# CGx2 coordinator handoff: review Wave 21, then plan Wave 22

> Historical Wave 21 snapshot. Its branch and dispatch instructions are retained for provenance. For current work, start with the [release completion ledger](plans/release-completion/README.md) and [main integration record](docs/branch-integration.md).

This is a snapshot for the next coordinator chat. It is **not** evidence that Wave 21 is complete. Start Wave 22 only after the user says the Wave 21 workers are done. Refresh branch, worktree, chat, test, and knowledge-graph state before acting.

## Project and current state

- Repository: `/home/john-riley/projects/lang`. Product: CGx2. Source `.cg`, notebook `.cgnb`, CLI `cgx2`, manifest `cgx2.toml`.
- Coordinator branch at handoff: `wave21/integration`, commit `6b79791` (`Plan Wave 21 independent CGx2 development lanes`). It descends from reviewed Wave 20 integration and coordinator repair `3478da2`. `main` has not received these wave commits. Do not merge to `main` without the user's review.
- [Progress](plans/progress.md) is **92% of the tracked first-release implementation scope after Wave 20**. This is a coarse feature inventory, not release readiness. The long-term OCaml-scale goal is below 10%. No Wave 21 implementation credit has been counted.
- Wave 20 passed combined offline Rust workspace tests, notebook UI loopback tests, 102 Python benchmark tests, benchmark readiness, formatting, Clippy with `-D warnings`, and a real pinned Torch 2.11 CPU `.cg` matrix call without Python as a middleman. The real facade was not sanitizer-instrumented. The coordinator fixed notebook overwrite-save after image deletion in `crates/cgx2-notebook-ui/src/lib.rs` and added HTTP regressions.
- Release blockers still include full supported-profile memory assurance, broad structured/source semantics, general library compatibility, source-level safe parallel/GPU lowering, live trusted solver evidence, scored benchmark isolation, and genuine paired LLM trials. The host previously lacked executable Z3/CVC5 and delegated CPU/memory/PID cgroups. Recheck availability rather than assuming it is still absent.
- An untracked `.serena/` directory existed in the coordinator checkout. Treat it as tool state; do not stage or delete it as part of a wave commit.

## Required reading and continuity

Follow `AGENTS.md`: call MegaMemory overview at chat start, query before reading source for project understanding, and record decisions/results after the task. Read [language design](docs/language-design.md), [validation plan](docs/validation-plan.md), [roadmap](plans/roadmap.md), [progress](plans/progress.md), [Wave 20 review](plans/wave20/README.md), [Wave 21 ledger](plans/wave21/README.md), and each relevant assigned `plan.md`. The user values explicit memory guarantees most, one readable syntax with structured tooling for LLMs, native notebooks, direct LibTorch interoperability, honest runtime and LLM benchmarks, and independent review of each wave.

## Wave 21 coding chats

Six user-owned chats were created from `6b79791` in separate worktrees. Locate their current chats/worktrees and exact commits; do not infer completion from this table or from a chat title.

| Lane | Assigned plan | Exclusive area |
| --- | --- | --- |
| Region transfer assurance | [plan](plans/wave21/region-transfer-assurance/plan.md) | Runtime, memory conformance, `cgx2c/tests/memory_release_*` |
| Nominal scalar records | [plan](plans/wave21/nominal-scalar-records/plan.md) | Core syntax/checker/CLI/Rust emitter/API/effect/package and `cgx2c` record paths |
| Direct float32 CPU matrices | [plan](plans/wave21/direct-f32-matrix/plan.md) | Binding and LibTorch crates |
| Pure local notebook imports | [plan](plans/wave21/notebook-pure-imports/plan.md) | Five notebook crates |
| Python-only numeric arrays | [plan](plans/wave21/python-numeric-arrays/plan.md) | Python bridge and foreign crates |
| Real matrix baselines | [plan](plans/wave21/matrix-runtime-baselines/plan.md) | `bench/**` |

Each worker was asked to complete one bounded coding deliverable, run its specified checks, commit only owned paths, report files/results/limitations/SHA, and stop. The plans were peer-criticked before dispatch. Key cross-lane contracts: records reuse existing `ExprKind::Name(Path)`/`Call` forms and canonical `Bool`; notebook imports must actually execute checked `module.function(...)` calls, not just validate package metadata; numeric arrays use shape-specific checked 8 MiB input/output and 16 MiB aggregate copied-payload budgets without relaxing generic bridge limits; matrix benchmarks separate cold startup/closure, adapter/copy, and warm compute and must not claim general speedup from tiny matrices.

## What to do when the user says Wave 21 is complete

1. **Inventory and review.** Use the chat/worktree tools and Git to identify all six worker commits and their real status. Compare each changed path with its exclusive plan and inspect actual code, tests, measurements, and omissions. If a lane is incomplete, report that precisely and decide whether a bounded repair is needed before integration. Do not count a skipped solver, cgroup, NumPy, or real Torch test as a pass. Keep worker branches untouched.
2. **Integrate safely.** Start a coordinator review branch from the common `6b79791` plan commit or verify that the current integration branch is still the intended base. Cherry-pick reviewed commits one at a time, resolving genuine interface conflicts in coordinator-owned integration commits. Update root `Cargo.lock` only here if needed. Keep `main` unchanged. Check API compatibility, especially record AST users outside its lane, notebook source execution, Python bridge size limits, and additive float32 ABI stability.
3. **Run combined gates.** Use the repository's pinned Rust toolchain and offline mode. At minimum: `cargo test --workspace --exclude cgx2-notebook-ui --offline -- --test-threads=1`, `cargo test -p cgx2-notebook-ui --offline` where localhost binding is permitted, `python3 -m unittest discover -s bench/tests -q`, `python3 bench/readiness.py --build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --offline -- -D warnings`, and `git diff --check`. Run targeted record interpreter/native debug/release, cross-region memory, copied-array CPython, and pinned real LibTorch float32/f64 tests if their dependencies are present. The previous host needed `RUSTUP_TOOLCHAIN=1.98.1`, `LIBRARY_PATH=/tmp/cgx2-python-lib`, and a project `TMPDIR` for some runs; verify paths/versions instead of copying them blindly. Real Torch tests can take minutes. Record exact commands, results, skips, and test scope.
4. **Judge evidence.** Read raw matrix benchmark observations and equivalent C++/Python baseline conditions. Distinguish cold path from warm compute. Do not infer GPU or general `.cg` parallel execution from standalone CPU policy tests. Do not infer LLM authoring gains from synthetic tasks; genuine paired model traces and independent task review are still required. A finite passing memory corpus is useful assurance, not a blanket safety proof.
5. **Update the ledger.** Add a Wave 21 review/result section to `plans/wave21/README.md`; update `plans/progress.md` and `plans/roadmap.md` only for demonstrated capability, keeping the percentage unchanged if hard gates remain open. Explain what is implemented, what is tested, and what blocks release. Commit coordinator fixes and documentation on the review/integration branch.
6. **Design Wave 22 from the gaps.** Give memory correctness first priority. Choose only tasks with stable input contracts and disjoint write paths. Create `plans/wave22/README.md` and one detailed `plan.md` per coding chat with scope, exclusive files, API contract, acceptance fixtures, offline checks, limitations, and stop-for-review handoff. Review the plan for interface seams and benchmark validity before dispatch. Commit it, create user-owned project worktree chats from the same verified Wave 22 plan commit, and report their IDs and the evidence-based progress percentage. Do not automate future waves or launch Wave 23.

The user asked for repeated parallel waves with manual review. Do not start Wave 22 work merely because this handoff exists; wait for the user's Wave 21 completion signal. After that signal, the review, integration, plan, and dispatch above are authorized by the established workflow.

## Prompt for the user to send to the new chat

> Wave 21 is complete. Follow `/home/john-riley/projects/lang/handoff.md`: review all six Wave 21 worker commits against their plans, integrate them on a review branch while keeping `main` unchanged, run the combined and real external checks that are available, update the evidence-based progress estimate, then write and peer-review detailed independent Wave 22 plans and launch the Wave 22 worktree chats. Stop after dispatch so I can review each worker's result. Report exact commits, checks, skips, plan links, chat IDs, and the progress percentage. If a Wave 21 lane is incomplete, handle or clearly report that before claiming integration.
