# Public scalar syntax pilot packet

This is a six-case **candidate packet**, not a release holdout or a model result. `candidate/indent` and `candidate/brace` are the only model-visible source trees. `packet.json` gives both variants the same API descriptions, example arguments, behavior descriptions, prompt, allowed tools, 8,192-token context budget, 4,096-token run budget, and 120-second time budget. `private/oracle.json` stays outside candidate trees. An operator must also keep traces, hidden checks, and any answer changes outside the model workspace.

Braces are experimental. `bench/syntax_packet.py` accepts only one-level function braces and semicolon-terminated scalar statements, writes canonical `.cg` to a temporary directory, and sends only canonical `.cg` to `cgx2c`. It compares exact canonical bytes, emitted Rust hashes (the compiler's parsed/lowered operations), and native acceptance outputs for each variant. Equal canonical bytes imply equal source semantic units for this narrow packet; source byte counts remain separate observations, not model token estimates. The private oracle comes from the existing native scalar pilot and is still awaiting independent human task review.

From the repository root, run the full readiness report with one command:

```sh
python3 bench/readiness.py --build
```

`toolchain-lock.json` freezes the Rust/Cargo executable SHA-256 values, versions, root `Cargo.lock` SHA-256, and `cgx2c` Git source tree. `packet-hashes.json` freezes the public task contract, both source variants and private oracle bytes. The report checks them against this checkout. The build recipe is local and reproducible under these pins; it does not attest another host's build.

External paired traces use `bench/schema/syntax-pilot-trace-v1.schema.json` and can be passed as repeated `--trace PATH` arguments. Each trace records exact model family/version, seed, provider usage record or local tokenizer implementation/version and token-ID digest, disjoint input/output/tool/diagnostic token counts, total, wall time, repair count, human interventions, **final submitted** source bytes and SHA-256, external verification reference and outcome. The validator rejects missing provenance, mismatched totals, duplicate conditions and wrong budgets. Its comparison reports output-token and total-token ratios separately for paired successes. Every failure consumes its full token and wall budget in all-task charges. It never estimates model tokens from source text. The external operator remains responsible for authenticating usage and verification references and measuring final source bytes against the pinned artifact.

The present packet has no independent human review, no model trials, and no delegated cgroup isolation on this host. Readiness therefore reports zero genuine paired traces and no LLM improvement claim. Existing `bench/isolation.py` controls and the closed-output-pipe wall timeout regression remain the scored execution path; this packet does not invoke candidate submissions.
