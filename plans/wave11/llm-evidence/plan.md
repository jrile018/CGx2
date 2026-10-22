# Paired LLM holdout statistics

> Work in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Make the benchmark report reflect tasks, repeated seeds, model families, failures, and actual model tokens rather than treating rows as independent observations. The result is a trustworthy analysis tool, not a claimed CGx2 win.

**Architecture:** Extend the existing paired holdout protocol and result validator under `bench/**`. Primary comparisons are matched task clusters by model family and condition; resampling keeps all repeated runs for one task together. Report verified completion, all-task cost-to-success charging failures their full budget, paired-success total tokens and wall time, output tokens separately, confidence intervals, and explicit noninferiority/superiority decisions against preregistered margins. Reject missing or duplicated task/model/seed pairs and incompatible tool/doc/hardware budgets before analysis.

**Tech stack:** Python 3 standard library and `bench/**` fixtures/tests; no new dependency or model invocation.

**Spec:** `docs/validation-plan.md` LLM authoring/comprehension gates and reporting rules; `bench/README.md` holdout protocol.

## Constraints

- Own only `bench/**`. Treat CGx2/Python/Rust/TypeScript/OCaml executables as black boxes; do not edit language crates, root manifests/lock, docs/plans. Do not generate fake model traces as evidence. Synthetic fixtures are marked synthetic and used only to test analysis code.
- Bootstrap at the **task** level, stratify by model family and repository tier, keep paired runs paired, and distinguish uncertainty across tasks from variation across seeds. Freeze seeds/resample count and the proposed five-percentage-point completion noninferiority plus 20% token/time improvement targets before reading genuine holdout outcomes. When data are missing or power is insufficient, report `incomplete` or inconclusive, not a passing percentage.
- Count failed attempts at their full allowed model token/time budgets in all-task cost-to-success. Paired-success metrics condition on both correct and are supplementary. Record actual tokenizer/model versions; lexical source token counts remain descriptive only. A task with hidden tests in the starter workspace or leaked gold change remains invalid.

## Tasks and checks

1. Add synthetic tests for task-clustered versus row bootstrap difference, completion ties/near-margin cases, mixed model families/tiers, failed attempts, missing pairs, leaked tests, and confidence-bound decisions. Demonstrate that the old IID-row method would overstate certainty on one clustered fixture.
2. Implement deterministic report JSON/text with raw task counts, exclusions, confidence intervals, and gate decisions. Keep old public calibration fixtures clearly labeled non-evidence. Provide a read-only command that accepts valid external trace paths; it must not launch models or open a holdout automatically.
3. Run `python3 -m unittest discover -s bench/tests` and at least one synthetic valid/invalid report smoke. Update `bench/README.md` with exact input fields, formulas, resampling seed/count, and interpretation. Commit only `bench/**`; report tests, limitations, absence/presence of genuine traces and SHA.

**Acceptance:** The harness can decide the preregistered LLM gates from valid paired data without pseudoreplication, while current no-trace fixtures remain inconclusive.
