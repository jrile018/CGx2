# LLM and Runtime Holdout Harness Implementation Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Work directly in this chat. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Make CGx2's claimed LLM coding and runtime benefits testable with paired, hidden-correctness tasks and explicit missing-evidence reports.

**Architecture:** Extend `bench/**` with a versioned task-matrix and trace-ingestion contract. Keep task statements and hidden tests separate; compare CGx2 with Python, Rust, TypeScript, and OCaml where each baseline is applicable. Calculate verified completion, time, tokens, cost-to-success and uncertainty from actual traces only. Keep native runtime microbenchmarks separately labeled from model-authoring and end-to-end application results.

**Tech stack:** Python 3 standard library, existing `bench/runner.py`, `report.py`, `pilot.py`, `native_pilot.py`, `matrix_audit.py`; offline tooling.

**Spec:** `docs/validation-plan.md` LLM and runtime gates; `bench/README.md`; `plans/wave9/README.md`.

## Global constraints

- Own only `bench/**` and this plan. Do not edit compiler crates, project-wide docs, or root manifests. Do not fabricate LLM responses, token counts, successful completions, CPU wins, PyTorch results, or confidence intervals from missing trials.
- Version each task/trace schema. Required trace fields: task ID and hidden-test version, language, model family/version, run ID, seed/order, allowed tools/documentation budget, prompt+completion tokens, wall time, final patch hash, hidden-test result, timeout/failure cause, and hardware/toolchain when measuring runtime. Inputs are bounded; invalid/duplicate/mixed-budget traces fail validation.
- Include both 3–10 file and 20–100 file repo tiers, with memory-heavy, modularity, foreign-library and comprehension/repair tasks. The initial fixtures may be calibration tasks, not secretly used as held-out evidence. Record applicability exclusions for a baseline before analysis, not after seeing results.
- Report the preregistered gates: verified completion within five percentage points of the strongest applicable baseline, >=20% lower paired-success total tokens and wall time, and all-task cost-to-success including failures at full budget. Show confidence intervals and a power/uncertainty warning. A 15% syntax token target is diagnostic only and cannot override memory correctness.

## Review focus

- Missing model traces must yield “incomplete,” never a win or a numeric completion rate.
- A task solved in CGx2 but failed in a baseline stays in all-task success and cost analysis, not paired-success speed.
- Hidden tests must not be readable from the agent starter tree.
- Mixed model/tool budgets or reused task IDs must fail audit rather than silently combine.
- Runtime results must include process startup and dispatch where the comparison says they do.

## Tasks

### Task 1: Matrix and trace schema

**Files:** `bench/schema/**`, `bench/fixtures/holdout/v1/**`, `bench/tests/**`, `bench/README.md`.

- [ ] Add failing tests for valid/invalid traces, missing runs, duplicate IDs, mismatched budgets, applicability exclusions, and hidden-test leakage.
- [ ] Define the versioned matrix and trace contracts, with at least one reviewed pilot task in each repo-size tier and an explicit path for later private hidden tests. Keep private answers out of agent-visible starter files.
- [ ] Run `python3 -m unittest discover -s bench/tests -p 'test_*.py'`.

### Task 2: Paired analysis and honest reporting

**Files:** `bench/matrix_audit.py`, `bench/report.py`, new focused analysis module and tests.

- [ ] Add failing tests for success noninferiority, paired-success token/time ratios, failure-at-full-budget cost, interval calculation, censored timeouts, and no-data/incomplete results.
- [ ] Implement deterministic analysis with exact formulas and readable JSON/Markdown output. Distinguish descriptive pilot statistics from preregistered holdout verdicts. Never infer a model win from the six-case language/runtime pilot.
- [ ] Run the audit on valid synthetic fixtures (to test math) and an intentionally incomplete real matrix (to verify it refuses a claim).

### Task 3: Reproducible run protocol

**Files:** `bench/runner.py`, `bench/native_pilot.py` only if necessary, fixture documentation and tests.

- [ ] Add a CLI/example that records external model traces without making a model call, checks patch hash and hidden-test version, then produces the paired report. If model access is available, run a clearly labeled small calibration trial; otherwise report that no model trial ran.
- [ ] Preserve existing six-case native parity tests and startup timing. Add at least one application-shaped runtime case with the same input/output oracle across available targets, or document why that gate remains unmeasured.
- [ ] Run all benchmark unittests, existing pilot parity, and the matrix audit; commit only owned paths with exact measured scope and missing evidence.

**Acceptance:** Another operator can run a fair, versioned paired experiment, audit its traces, and see an honest incomplete report until real held-out model and runtime evidence exist.
