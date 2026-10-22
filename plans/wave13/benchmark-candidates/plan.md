# Executable benchmark task candidates and review gate

> Use the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Replace placeholder task-pack entries with a small, executable, language-neutral candidate set that can receive independent review before any LLM claim. This wave creates benchmark material and validation; it does not generate or imply model performance results.

**Architecture:** Use the Wave 12 v3 task/run schemas and importer. Prepare at least one meaningful 3–10-file task and one meaningful 20–100-file task using only frozen Wave 12 CGx2 behavior, each with a comparable baseline implementation or starter repository in the preregistered applicable language(s), a private acceptance suite outside model-visible workspaces, a blinded rubric for comprehension or feature work, and content hashes. Keep task prompts equivalent across languages and select the strongest applicable baseline before any trace exists. Mark each task `candidate` until reviewers other than its author approve the task, equivalence, hidden tests and baseline choice; no fabricated reviewer IDs or model traces.

**Ownership:** Only `bench/**`. Do not edit source/runtime/notebook/compiler crates, root manifests/lock, docs or plans. Use existing `cgx2`, Rust and Python executables as frozen dependencies. If a large-tier task cannot be meaningful with Wave 12's limited module/profile support, provide an explicit blocked candidate with a precise language dependency instead of adding filler files.

## Required behavior

1. Each candidate contains exact source snapshot hashes, task category/tier, language-neutral acceptance criteria, real build/run commands, allowed tools/docs, a hidden-test reference with independently executable validation, memory/foreign/syntax feature scope, and expected correctness outcomes. Negative fixtures must fail for the intended reason, not simply fail to compile.
2. Add a validator that checks repository file counts, source/build reproducibility, hidden-test isolation, baseline/CGx2 task equivalence metadata, no CGx2 self-baseline, missing or invented review fields, stale hashes, and required v3 run provenance. It must refuse a `frozen` status without verifiable independent review records. Failures return `insufficient_evidence`, never a zero-cost or favorable result.
3. Run the task correctness tests against both CGx2 and selected baseline on this host where tools are installed. Record exact versions and skip reasons; do not time incomparable programs or call a pilot a release benchmark. Do not invoke a model or synthesize traces to fill the holdout grid.
4. Update `bench/README.md` with reviewer checklist, candidate IDs, commands, known equivalence limits, and the path to a later 60–100-task holdout with two model families and paired repeats.

## Verification and handoff

Run `python3 -m unittest discover -s bench/tests -q`, schema/task-pack validation, and both task acceptance suites. Commit owned paths and report SHA, executable task count/tier/baseline, reviewer work still needed, and a truthful `insufficient_evidence` report.

**Acceptance:** The candidate set has real runnable correctness checks and cannot be frozen or scored without independent review and genuine paired runs.
