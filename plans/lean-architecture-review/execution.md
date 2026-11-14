# First checkpoint execution DAG

**Goal:** deliver the confirmed plan's frozen architecture inputs, architecture/ownership claim map, actual Rust-to-Lean feasibility result, and CPU/GPU readiness evidence.
**Architecture:** one coordinator and three independent leaf workers on an isolated execution branch. Workers write only their owned evidence directories; compiler sources remain frozen during the first checkpoint.
**Tech stack:** Rust, Lean/source extraction where feasible, Python evidence tooling, CPU/GPU runtime probes.
**Spec:** [confirmed plan](plan.md), authorized for parallel execution on 2026-11-13.

## Global constraints

- Repository: `/home/john-riley/.codex/worktrees/github-main-consolidation/lang`; branch `riley/lean-architecture-review-execution`.
- Original `5185` worktree is read-only. Frozen 15-file ownership overlay: `/tmp/cgx2-architecture-review-inputs-20261113/wip-overlay`.
- All workers consume `research/architecture-review/input-manifest.json`, committed source and the frozen overlay.
- No changes to crates, Cargo manifests/locks, existing formal proof package, original worktrees, main, or another worker's output.
- No worker commits, pushes, delegates, or edits the shared plan/ledger. Coordinator integrates and publishes coherent verified milestones.
- Isolated open-source tooling under lane-specific `/tmp` paths is allowed. No new paid services, system-driver/security changes or mutations to existing shared Python/opam environments.
- Build-tool jobs are limited to two per lane. Readiness smoke tests are not performance measurements; no benchmark claim while setup runs contend for hardware.

## Split and coordination analysis

Tactical split: three agents inside one coordinator's lane. Architecture mapping, proof-tool feasibility and environment readiness have stable read-only source inputs and disjoint outputs. Their implementation details are hidden behind checked reports; they do not change runtime interfaces. One receipt review per lane plus one integrated verification is expected. Reserve roughly 10–20% of this checkpoint's effort for coordination/review; this is a planning estimate, not a measured speedup claim. No branch merges or pairwise worker synchronization are required. Actual semantic optimization remains sequential behind evidence and user tradeoff decisions.

## Review focus

- Unfinished ownership code must not become accepted source support by being described or snapshotted.
- A hand-transcribed Lean model must not be relabeled as actual-source verification.
- Missing GPU device/runtime support must remain a blocker, never CPU fallback counted as GPU success.
- Interpreter/package drift and absent native closures must be validated rather than inherited as historical passes.
- Runtime diagnostic changes and counter overflow must not disappear inside an overly strong failure-atomicity claim.

### T0: Freeze inputs

- depends_on: []
- owns: [`research/architecture-review/input-manifest.json`, frozen input snapshot]
- non_goals: [modify original WIP, accept unfinished features]
- consumes: [plan commit 4ed1718, current original WIP]
- produces: [baseline/source identities and stable 15-file overlay]
- acceptance: [all copied WIP bytes match originals before/after copy; 518 baseline files hashed]
- verification: `python3 research/architecture-review/verify.py --inputs-only` (coordinator creates verifier)
- estimate: S
- coordination_risk: low
- status: complete

### T1: Architecture and ownership claim map

- depends_on: [T0]
- owns: [`research/architecture-review/architecture/**`]
- non_goals: [production edits, benchmark conclusions, proof setup]
- consumes: [frozen inputs, confirmed plan and active ownership plan]
- produces: [`report.md`, `claims.json`, `result.json`; exact path/line/source-variant anchors]
- acceptance: [complete intended pipeline mapped; WIP separate; >=10 scoped claims with assumptions/obligations; concrete collector and source-authentication seams]
- verification: coordinator validates source anchors and claim fields, then integrated verifier
- estimate: M
- coordination_risk: low
- status: complete

### T2: Actual-source-to-Lean feasibility

- depends_on: [T0]
- owns: [`research/architecture-review/proof/**`, `/tmp/cgx2-arch-review-proof/**`]
- non_goals: [hand-copy claimed as source proof, crate refactor, whole compiler proof, environment lane tooling]
- consumes: [actual budget kernel, frozen source hashes, existing Lean reference model]
- produces: [`report.md`, `result.json`, pinned setup/extraction attempt evidence, reproducible checking command; generated translation/proof if feasible]
- acceptance: [attempt a documented supported source extraction route with actual kernel or record concrete attempted-tool/operation incompatibility and alternatives; no mere PATH absence as final finding]
- verification: exact worker-provided offline evidence/check command, independently rerun by coordinator where prerequisites permit
- estimate: L
- coordination_risk: moderate
- status: complete

### T3: CPU/GPU readiness

- depends_on: [T0]
- owns: [`research/architecture-review/environment/**`, `/tmp/cgx2-arch-review-environment/**`]
- non_goals: [timed superiority claims, driver/security changes, paid/cloud resources, modifying historic environments]
- consumes: [local machine/toolchain and historical dependency references]
- produces: [`report.md`, `result.json`, version/device/pin evidence, CPU smoke fixtures and genuine GPU smoke or exact blocker]
- acceptance: [Rust/OCaml/Python tested where available; coherent ML dependency readiness checked; GPU hardware distinguished from device execution; blockers have concrete safe remedies]
- verification: exact worker-provided probe/check command and raw logs, independently rerun by coordinator
- estimate: M
- coordination_risk: moderate
- status: complete

### T3R: Version-specific GPU remedy discovery

- depends_on: [T3]
- owns: [`research/architecture-review/environment/gpu-remedies.*`, new environment raw metadata]
- non_goals: [GPU wheel downloads, package/driver/security changes, GPU ML success claims]
- consumes: [T3 device/backend evidence, official version-specific support and package metadata]
- produces: [host/driver distinction, concrete isolated user-space option, compressed sizes and unresolved storage/compatibility gates]
- acceptance: [match exact OS/kernel/GPU rows; distinguish current and legacy releases; do not infer required driver replacement from missing HIP; retain unknown unpacked/closure size]
- verification: environment check plus coordinator SHA-256/size checks of seven new raw evidence files
- estimate: S
- coordination_risk: low
- status: complete

### T4: Coordinator synthesis and verification

- depends_on: [T1, T2, T3, T3R, T5]
- owns: [`research/architecture-review/README.md`, `research/architecture-review/verify.py`, `research/architecture-review/verification/**`, shared task status and plan]
- non_goals: [overclaim partial results, merge main, choose substantive tradeoffs silently]
- consumes: [all reviewed lane receipts]
- produces: [verified checkpoint report, prioritized actual findings/options, evidence reproduction command]
- acceptance: [source integrity; all claims scoped; every reported pass backed by executed evidence; GPU and proof blockers explicit]
- verification: `python3 research/architecture-review/verify.py` and `git diff --check`
- estimate: S
- coordination_risk: low
- status: complete

### T5: Comparative fixture/study protocol

- depends_on: [T1, T3]
- owns: [`research/architecture-review/comparisons/**`]
- non_goals: [paid model trials, fictitious speedups, performance timing during setup contention]
- consumes: [architecture capabilities/gaps and environment readiness]
- produces: [case-by-case semantic contracts and paired idiomatic/equipped CPU/GPU evaluation matrix; no-spend authoring-study specification]
- acceptance: [all three application tracks and four languages represented; unsupported routes explicit; correctness/failure checks precede timing]
- verification: integrated verifier checks workload matrix and evidence states
- estimate: M
- coordination_risk: low
- status: complete

## Worker receipt contract

Each lane's result.json contains `status` (`complete`, `partial`, or `blocked`), `scope`, `checks` (objects with `id`, `status` (`pass`, `fail`, `blocked`, `not_run`), `command`, `exit_code`, `evidence_file`, `scope`), and `blockers` (objects with `id`, `description`, `remedies`). File paths are relative to the lane directory unless explicitly recorded as external raw tool artifacts. A pass needs an actual successful command/log, not historic provenance alone. Explicitly unrun checks may have a null command. Expected-failure negative controls document their expected nonzero exit independently from ordinary successful execution. Architecture claims use `id`, `name`, `status`, `scope`, `source_refs`, `assumptions`, `obligations`, and `evidence`; statuses distinguish intended, implemented, implemented_unaccepted, historically_tested, model_proved, implementation_connected, and blocked.

## Integration protocol

Workers return completed receipts without committing. Coordinator reviews diffs, verifies exact evidence, updates status and starts newly ready successors without a whole-wave barrier. All milestone claims use fresh checks. A blocked GPU/source-translation row is retained in the final report and informs a concrete user choice; it does not close the overall architecture/comparison objective.

## Integrated checkpoint outcome

T0–T5 and the T3R metadata follow-up are complete as bounded deliverables. T2 and T3 receipts remain partial for full-runtime proof and CGx2/GPU-ML integration. Coordinator independently replayed actual source extraction, generated Lean and six lemmas plus the expected-failure source mutation; CPU/GPU readiness replay returned13pass/1blocked. Architecture and comparison provenance/schema verifiers passed. Staged whitespace checking passes for all nonraw files; original pinned terminal logs intentionally retain CR progress/blank-end-line warnings, recorded in verification/whitespace.json. These results do not complete the broader plan, establish superiority or authorize a production redesign. No production source or main changes were made. The checkpoint is committed for user review before another bounded implementation task.
