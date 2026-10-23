# Blinded paired LLM authoring pilot

**Goal.** Determine whether CGx2's readable indent syntax helps an LLM produce correct code with fewer output tokens and repairs than a brace variant. The Wave 17 packet now gives public `TODO()` starters and keeps complete references private; this wave makes that separation operational.

**Ownership.** Edit only `bench/**`. Do not edit language/compiler crates, root Cargo files, shared docs, or another lane.

## Deliverable

1. Add a deterministic export command that copies only the public prompt, one assigned starter, shared tool wrapper/contract, and permitted acceptance interface into an isolated trial directory. Exclude `private/oracle.json`, `private/reference/**`, packet validation internals, and the other variant's completed answer. Reject symlinks/path traversal and verify exported file hashes. Document what the model can see and how the operator keeps private grading separate.
2. Pre-register order balance and measurement: same model/settings, six task pairs, equal context and time/token budgets, actual provider output-token accounting, repair count, elapsed wall time, compiler/test success, and pair-level uncertainty. Validate trace schema and reject fabricated/missing usage data, failed isolation, or a trace whose visible context contains private references.
3. If a genuine clean model interface and usage telemetry are available, run a small paired pilot through the public export and grade in a separate process, preserving exact anonymized traces. Otherwise emit an explicit `unrun` readiness report with zero genuine pairs and no improvement claim. Do not treat fixture token counts or self-written references as measured LLM productivity.
4. Keep the source pilot's two syntax variants semantically equivalent using the shared normalizing wrapper. Record that results cover this bounded six-task scalar slice, not a whole-language speed or quality claim. If cgroup controllers remain undelegated, keep scored candidate isolation unsupported and do not bypass that gate silently.

## Verification and handoff

Run `python3 -m unittest discover -s bench/tests -q`, packet compiler parity/readiness, export leak tests, and `git diff --check`. If a scored trial cannot run on this host, show the exact missing capability and still deliver the public-only export. Commit only owned paths. Report commit SHA, changed files, test results, count of genuine paired traces, and precise status of improvement/isolation claims; stop for review.
