# LLM syntax and task pilot readiness

**Goal.** Make the promised LLM ease and shorter-code claims measurable without treating static token counts or mock traces as a model win. Human task review and the isolation host remain external gates.

**Ownership.** Only `bench/**`. Do not edit compiler/runtime/notebooks, root Cargo files, shared docs, or other lanes. Do not expose hidden task answers to candidate workspaces.

## Deliverable

1. Prepare a small independently reviewable paired syntax packet from the currently executable scalar subset: indentation/newline and experimental brace variants with identical APIs, examples and language-neutral expected behavior. A test-only normalizer must map each brace variant to canonical `.cg` before checking or running it; verify the resulting parsed operations and acceptance outputs match the indentation source. This does not make braces a supported CGx2 syntax. Validate equal context budgets, constraints and semantic units. Keep candidate-visible files separate from hidden oracle material. The packet is for a pilot, not a release holdout.
2. Extend trace validation to record model family/version, provider usage provenance, tokenizer implementation/version where local tokenization is used, input/output/tool/diagnostic tokens, elapsed wall time, repair iterations, human interventions, source bytes and verified outcome for each paired run. Compute output-token and total-token comparisons separately, charge failed attempts their full budget, and reject missing provenance or inconsistent totals. Do not estimate actual model tokens from whitespace or source length.
3. Produce a one-command readiness report listing independent human review status, a reproducible compiler build recipe and frozen toolchain hashes, isolation probe, task equivalence checks, and count of genuine paired traces. With no model trials or delegated cgroups, report `unrun`/`unsupported_isolation` and no improvement claim. Preserve the Wave 16 cgroup and wall-time protections, including closed-output-pipe regression.

## Verification

Run Python benchmark unit tests, schema/hash checks, candidate-isolation probe, and paired packet validation. State exact unrun gates, any measured pilot-only quantities, and commit ID. Stop for review.
