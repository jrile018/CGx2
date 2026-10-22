# Candidate task review and measurement packet

Use the attached worktree; read shared Wave 14 references and this plan. Commit only owned paths and stop for review. Do not manufacture model runs or promote unreviewed candidates to benchmark results.

**Goal.** Turn the two Wave 13 executable task candidates into an independently reviewable packet with workspace-bound hidden acceptance, a freeze decision trail, and an exact protocol for paired model/runtime evidence. The reviewer, not this worker, decides whether to freeze each task.

**Ownership.** Only `bench/**`. Do not edit compiler/runtime/notebook code, root manifests, shared docs, plans, or coordinator ledger. Keep private reference solutions out of public task prompts and scored model context.

## Deliverable

1. For each candidate, provide a concise task card: independent user need, allowed files/API, completion criteria, hidden cases, edge cases, seed/data provenance, and why the task is not a tiny syntax translation. Add a structured reviewer checklist with fields for ambiguity, leakage, difficulty, corpus duplication, and accept/reject rationale; leave human review decisions blank.
2. Harden `accept.py` so every scored submitted workspace is exactly the supplied candidate path and language, separated from bundled starter/reference. Test acceptance of a correct overlay and rejection of starter, wrong language, symlinks, private fixture access, and a candidate that hard-codes public examples but fails hidden cases. Keep resource/time limits and output normalization explicit.
3. Add an evidence packet generator that records immutable task and harness digests, model family/version, prompt budget, model sampling settings, trial/run IDs, workspace hash, pass/fail, token count, latency, compile/run time, and paired baseline link. It may consume real external traces later but must emit an honest `unrun` state now. Never infer a speed or LLM improvement from local reference runs.
4. Document the preregistered comparison and uncertainty reporting for human-written CGx2, Rust/Python baselines, and at least two independent model families. Separate language authoring time, generated token count, compile/run success, runtime speed, and memory safety outcomes.

## Verification

Run `python3 -m unittest discover -s bench/tests -q`, a candidate workspace smoke for both languages of each task, and all evidence validators. Report pending human review, absent model trials, exact fixtures, and commit SHA.

**Acceptance.** A reviewer can approve or reject each candidate using complete artifacts, and the pipeline cannot call an unrun or unreviewed task a benchmark win.
