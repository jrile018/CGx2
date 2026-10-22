# CGx2 Benchmark Readiness and Native Pilot Plan

> Implement this plan in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Measure the new native scalar path fairly at its current scope and make it hard to mistake incomplete LLM task matrices for evidence that CGx2 is easier or faster to write.

**Architecture:** Extend the standard-library-only `bench` harness. Add a small versioned native scalar parity/timing pilot with hand-written equivalent Rust references for the existing six CGx2 cases where applicable. Add a task-matrix audit command that checks real authoring/comprehension benchmark manifests, starter repository tiers, hidden-test isolation, paired conditions, provenance, and trace coverage before a holdout is run. Outputs are machine-readable and preserve failures/missing data.

**Tech stack:** Python standard library, installed Rust 1.98/rustc, existing `cgx2c` and benchmark schemas. No model invocation or external dataset download.

**Specification:** `docs/validation-plan.md` LLM/runtime gates, `bench/README.md`, `bench/pilot.py`, `plans/wave7/README.md`.

## Owned paths and boundaries

- Own only `bench/**` and this plan. Do not edit Rust crates, docs outside `bench`, root files, or other lanes. Consume the Wave 6 `cgx2c build` and scalar CLI commands. A new Wave 7 import or Python feature is outside this frozen pilot.
- Native pilot: for supported scalar cases, compare exact stdout, exit status, and declared error class before timing. Hand-written Rust must use checked i64 arithmetic and the same argument/Unicode behavior; exclusions are explicit with reasons. Build both source sets with pinned `rustc` command/profile; report hashes and compiler versions. Separate build duration, cold subprocess launch, and repeated launch samples. Randomize or alternate order, record every raw sample, timeout, CPU/host/thread settings, and summary uncertainty. Tiny scalar binaries are startup-heavy; label them as a toolchain smoke and do not claim the owned/managed runtime reaches the Rust target.
- Matrix audit: read trusted task manifests/results and verify `small` starter repositories have 3–10 files and `large` have 20–100, based on a documented file-count rule. Check unique task/condition/model/seed keys, balanced paired variants, pinned versions and budgets, hidden acceptance tests outside model workspaces, source/history leakage markers, real trace fields, and failed-run full budget charges. Report `ready`, `incomplete`, and `invalid` separately. Do not fabricate model traces, tokens, or task wins. An audit-ready matrix is not itself a valid holdout; human equivalence review remains required.
- Keep `bench/fixtures/sample-results` labeled synthetic and do not mix them into a CGx2 result. Any new example task is public calibration material, not a hidden holdout. Preserve old pilot commands and schemas or add an explicit versioned migration.

## Tasks

### Native scalar pilot

- [ ] Add red tests for parity failures, compiler failure, timeout, missing reference, order recording, and raw-sample preservation.
- [ ] Add hand-reviewed Rust counterparts to at least four of the six frozen scalar cases; run `cgx2c` and `rustc` builds before timing.
- [ ] Produce one real local pilot JSON and summarize only what its workload measures; do not commit transient host output unless clearly labeled as one machine's observation.

### Task-matrix audit

- [ ] Add red tests for an incomplete paired matrix, duplicate condition key, wrong repository size, hidden test leak, missing model token trace, and invalid provenance.
- [ ] Implement the audit command and a reviewer-facing checklist/JSON summary. Include a public calibration example that fails at least one gate for demonstration.
- [ ] Document how to freeze a future 60–100 task holdout, model families, seeds, prompt/tools budget, and independent review without implying those runs exist now.

### Verification and handoff

- [ ] Run `python3 -m unittest discover -s bench/tests`, existing `python3 bench/pilot.py parity --build`, the new native pilot, and the matrix audit examples.
- [ ] Commit only `bench/**` and this plan; report actual measured scope, missing traces/tasks, and unresolved release targets.

**Acceptance:** Native scalar parity/timing is recorded honestly, and the audit rejects incomplete or contaminated LLM comparison matrices before any productivity claim.
