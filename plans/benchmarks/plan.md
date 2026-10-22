# CGx2 Benchmark Harness Implementation Plan

> **For agentic workers:** Use the executing-plans skill task by task. This chat owns one bounded deliverable; do not spawn another agent, edit shared files, merge, or start a second assignment. Stop for user review after verification.

**Goal:** Build an on-demand, reproducible benchmark runner and report pipeline for the CGx2 hypothesis without claiming an unbuilt language has won.

**Architecture:** JSON task manifests define equivalent language variants, commands as argument arrays, limits, and hidden-test entry points. The runner launches a single case in a bounded subprocess, captures outcome and timing, and ingests model token usage from a provided trace. A report script checks pairability, computes success/time/token summaries, charges failures the full budget for cost-to-success, and labels missing data rather than zero-filling.

**Tech Stack:** Python 3.14.4 standard library only (`subprocess`, `json`, `statistics`, `unittest`).

**Spec:** `docs/validation-plan.md` and the syntax comparison requirement in `docs/language-design.md`.

## Global constraints

- Own only `bench/**` and `plans/benchmarks/plan.md`. Do not edit CGx2 compiler/runtime crates or shared docs.
- This is an **invoked** tool, not a scheduled automation. It must not call a model or install packages by itself.
- A six-task pilot is for harness calibration. No release conclusion may be emitted from it.
- Commands in manifests are argv arrays, never shell strings. Environment variables and dependency versions are recorded; secrets are omitted from reports.
- Missing token traces and incomplete cases are marked missing; never count them as zero tokens or successful completions.

## Public contract

Create `bench/README.md`, `bench/schema/task.schema.json`, `bench/schema/result.schema.json`, `bench/runner.py`, `bench/report.py`, `bench/tests/test_runner.py`, `bench/tests/test_report.py`, and small public fixtures under `bench/fixtures/`.

A task manifest records `task_id`, `tier` (`small` for 3–10 files, `large` for 20–100), `language`, `variant` (including syntax candidate when relevant), `argv`, `cwd`, `timeout_seconds`, `seed`, `expected_exit`, `hidden_test_argv`, and budget/version metadata. A result records wall time, exit status, stdout/stderr hashes, verified completion, iterations, output/total tokens when supplied, human interventions, failure reason, and provenance.

Expose CLI commands `python3 bench/runner.py --manifest PATH --out PATH` and `python3 bench/report.py --results DIR --out PATH`. Do not expose hidden test source through the result JSON.

## Review focus

1. Timeout must terminate the child process and record failure at the full time budget.
2. A malformed or shell-like command manifest must be rejected, never executed through a shell.
3. Missing token usage must be `null`/missing, not `0`.
4. A failed task must be charged its full budget in cost-to-success even if it exited early.
5. Only matched task/model/seed/language variants may enter a paired comparison.

## Tasks

### T1: Schema and bounded runner

- depends_on: []
- owns: [`bench/**`]
- non_goals: [actually invoking LLM APIs, CGx2 compiler implementation]
- consumes: [validation plan metrics]
- produces: [manifest/result schema and invoked runner]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Create failing unit tests for valid command, nonzero exit, timeout, missing executable, malformed argv, hidden-test failure, and provenance capture.
- [ ] Run `python3 -m unittest discover -s bench/tests -v` to see the new tests fail before implementation.
- [ ] Implement JSON validation and subprocess execution with `shell=False`, bounded timeout, output hashing, and one JSON result per run.
- [ ] Run the unit tests and a documented public fixture; verify no hidden test contents appear in results.

### T2: Paired reporting and syntax metrics

- depends_on: [T1]
- owns: [`bench/**`]
- non_goals: [statistical superiority claim from tiny samples]
- consumes: [result JSON]
- produces: [paired report and token/time accounting]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Add failing tests for matched pairs, unmatched rejection, missing-token handling, full-budget failure charge, output-vs-total token separation, and syntax candidate labels.
- [ ] Run tests to observe missing report behavior.
- [ ] Implement summary tables and machine-readable JSON report. For at least 30 matched pairs, compute a deterministic paired bootstrap 95% interval with 10,000 resamples and seed 0 for completion difference and time/token ratios; below 30 pairs label the interval `insufficient_data`. Never fabricate significance.
- [ ] Run tests and inspect a fixture report containing a success, failure, and missing-token case.

### T3: Public pilot fixtures and review handoff

- depends_on: [T2]
- owns: [`bench/**`, `plans/benchmarks/plan.md`]
- non_goals: [publishing holdout tasks, defining final benchmark outcome]
- consumes: [runner/report]
- produces: [reproducible public smoke fixture and documentation]
- estimate: S
- coordination_risk: low
- status: pending

- [ ] Add equivalent tiny Python and Rust smoke cases or clearly separate language-neutral fixtures, plus indentation/brace source samples for future model trials. Mark all smoke cases as non-evidence.
- [ ] Run `python3 -m unittest discover -s bench/tests -v`, `python3 bench/runner.py --manifest bench/fixtures/smoke.json --out /tmp/cgx2-smoke-result.json`, and `python3 bench/report.py --results bench/fixtures/sample-results --out /tmp/cgx2-smoke-report.json`; ensure commands are documented and repeatable.
- [ ] Document how a reviewer supplies fresh tasks, model traces, hidden tests, and frozen versions; report the absence of actual CGx2/model results.
- [ ] Commit owned paths and stop. Report commands/results, commit ID, data limitations, and any schema change proposal.

**Acceptance:** runner and report unit tests pass with stdlib only; malformed commands are safe, failures are charged correctly, and sample reports never imply CGx2 has been benchmarked.
