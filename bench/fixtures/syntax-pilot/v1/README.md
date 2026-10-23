# Public scalar syntax pilot packet

## Wave 18 public export and trial boundary

From the repository root, export exactly one assigned condition into a new directory:

```sh
python3 bench/syntax_export.py --task precedence --variant indent --out /tmp/cgx2-precedence-indent
```

The directory contains `prompt.json`, `starter.cg`, `syntax_tool.py`,
`syntax_normalize.py`, `acceptance-interface.json`, and `export-manifest.json`.
The exporter checks frozen packet and starter hashes, rejects source path
traversal and symlinks, and verifies every exported byte after writing. Never
mount this source packet or the repository in a model workspace. Give the model
only the exported directory and a pinned `cgx2c` through the exported wrapper.
The same wrapper normalizes both variants to the narrow canonical `.cg` form.
The operator copies final `solution.cg` and exact transcript out of that
workspace, then runs private acceptance in a separate isolated grading process.
No private oracle, complete reference, validation internals, or alternative
variant answer is exported. The public acceptance interface names the final
submission and grading boundary without exposing expected outputs.

`trial-protocol.json` freezes six task pairs in a three/three first-condition
balance, shared 8,192 context, 4,096 token, and 120 second budgets, and a
task-bootstrap uncertainty method. Its model registration remains empty. Before
any model call, an operator must freeze one model family/version, identical
temperature, top-p, max-output setting and seed for all twelve runs, and the
protocol hash. Record actual provider usage for every call, elapsed wall time,
repair iterations, exact model-visible files and transcript, final source,
compiler and hidden-test outcomes, and external verification references. The
v2 schema and `bench/syntax_trial.py` reject missing or inconsistent provider
usage, leaked private input, changed visible files, over-budget runs, and
failed isolation. An operator must independently authenticate provider and
isolation receipts; JSON booleans alone are not proof. A complete authenticated
six-pair grid is required before descriptive ratios or uncertainty are reported.
Each failure incurs the full token and wall budget in all-task cost. This small
public scalar slice cannot establish whole-language improvement.

`evidence-unrun.json` records the current zero-trial outcome. The local scored
candidate isolation probe reports undelegated `cpu`, `memory`, and `pids`
cgroup controllers. No provider trial or independent model receipt is available
in this lane, so there is no productivity claim.

This is a six-case **candidate packet**, not a release holdout or a model result. `candidate/indent` and `candidate/brace` contain incomplete `TODO()` starters and are the only model-visible source trees. `private/reference` contains complete paired solutions and `private/oracle.json` contains acceptance outputs; neither may enter a model workspace. `packet.json` gives both variants the same API descriptions, example arguments, behavior descriptions, prompt, allowed tools, 8,192-token context budget, 4,096-token run budget, and 120-second time budget. An operator must keep traces, hidden checks, reference solutions, and answer changes outside the model workspace. Copy only the chosen starter and a public task description into each isolated workspace; do not mount this packet directory wholesale.

Braces are experimental. `bench/syntax_packet.py` accepts only one-level function braces and semicolon-terminated scalar statements. It checks that the public starters normalize to identical canonical bytes, contain only API headers and unresolved placeholders, and differ from the private complete references. It separately normalizes the private references, compares their emitted Rust hashes, and checks native acceptance outputs for each variant. Equal canonical bytes imply equal parsed operations for this narrow packet; starter source byte counts remain separate observations, not model token estimates. The private oracle comes from the existing native scalar pilot and is still awaiting independent human task review.

For candidate tool use, the operator supplies the same `syntax-pilot` command alias for both variants, backed by `bench/syntax_tool.py` outside the candidate workspace. For example, the alias can invoke `python3 bench/syntax_tool.py emit-rust SOURCE.cg --variant brace --compiler PATH/TO/cgx2c --out OUT.rs`, or use `build` and `--out OUT` to produce a binary. The wrapper normalizes brace source before invoking `cgx2c`; direct `cgx2c` calls on brace source are unsupported. The operator must fix the alias, compiler, version, documentation budget, and tools before paired runs. The wrapper has no access to private reference files or oracle data.

From the repository root, run the full readiness report with one command:

```sh
python3 bench/readiness.py --build
```

`toolchain-lock.json` freezes the Rust/Cargo executable SHA-256 values, versions, root `Cargo.lock` SHA-256, and `cgx2c` Git source tree. `packet-hashes.json` freezes the public task contract, both starter and private reference variants, and private oracle bytes. The report checks them against this checkout. The build recipe is local and reproducible under these pins; it does not attest another host's build.

External paired traces use `bench/schema/syntax-pilot-trace-v1.schema.json` and can be passed as repeated `--trace PATH` arguments. Each trace records exact model family/version, seed, provider usage record or local tokenizer implementation/version and token-ID digest, disjoint input/output/tool/diagnostic token counts, total, wall time, repair count, human interventions, **final submitted** source bytes and SHA-256, external verification reference and outcome. The validator rejects missing provenance, mismatched totals, duplicate conditions and wrong budgets. Its comparison reports output-token and total-token ratios separately for paired successes. Every failure consumes its full token and wall budget in all-task charges. It never estimates model tokens from source text. The external operator remains responsible for authenticating usage and verification references and measuring final source bytes against the pinned artifact.

The present packet has no independent human review, no model trials, and no delegated cgroup isolation on this host. Readiness therefore reports zero genuine paired traces and no LLM improvement claim. Existing `bench/isolation.py` controls and the closed-output-pipe wall timeout regression remain the scored execution path; this packet does not invoke candidate submissions.
