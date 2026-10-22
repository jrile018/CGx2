# Freeze task definitions and run provenance for LLM evidence

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Make the Wave 11 task-clustered holdout analysis usable with auditable CGx2 development tasks and complete run metadata, while making no LLM speed claim without genuine paired model runs.

**Architecture:** Extend the benchmark manifest/validation tools under `bench/**` with immutable task IDs, tier, category, language-neutral acceptance criteria, baseline-language mapping, hidden-test references, repository snapshot IDs, prompt/tool/model version, tokenizer, time and token budgets, randomization seed, and exact CGx2/compiler/dependency hashes. Keep raw traces separate from derived metrics. An importer validates required fields, pair completeness, chronology, run failures and hidden-test isolation before the existing analysis consumes a run. No fake model traces or favorable baseline selection.

**Ownership:** Only `bench/**`. Read docs and frozen Wave 11 executables as needed; do not edit source/runtime/notebook/compiler crates, root manifests/lock, docs or plans. New tasks may be planning fixtures, but label them `candidate` until independently reviewed and frozen. Do not inspect or place hidden acceptance tests in a model-visible task workspace.

## Required behavior

1. Build a small candidate pack covering memory-sensitive change, package/API use, native C boundary, pure notebook comprehension, proof result interpretation, and numerical edge cases. Include both 3–10-file and 20–100-file repository tiers where feasible; a task that lacks a real equivalent baseline or independent review remains ineligible for a holdout claim. Specify strongest applicable baseline before results exist.
2. Add machine checks for every prerequisite in the validation plan: at least two model families for an eventual release report, repeated paired runs, frozen versions/prompts/tools/locks, actual tokenizer output plus total token accounting, verified completion, failures charged full budgets, task-clustered confidence intervals, and separate CGx2 versus Python/Rust/TypeScript/OCaml reporting. Missing evidence must produce `insufficient_evidence`, never zero cost or inferred success.
3. Test validation with deliberately incomplete, duplicate, leaking, mismatched and failed-run fixtures. Preserve Wave 11 analysis behavior and schema versioning; record migration for any new required field. Include a command that summarizes counts and exclusions before computing any result.
4. Update `bench/README.md` with how a human freezes candidate tasks and imports genuine runs, plus exact output interpretation. Do not claim task-pack creation is evidence of an LLM advantage.

## Verification and handoff

Run `python3 -m unittest discover -s bench/tests -q` and any bench schema/CLI smoke described in the updated README. Commit owned paths and report SHA, validated candidate counts/tier coverage, missing review/run requirements and a truthful `insufficient_evidence` example.

**Acceptance:** The harness refuses an unsupported superiority conclusion and is ready to ingest real, independently reviewed paired task runs without changing the preregistered scoring after results arrive.
