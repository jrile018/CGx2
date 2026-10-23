# Cross-region transfer and alias assurance

**Goal.** Advance the highest-priority memory release gate with explicit cross-region, alias, and close-path evidence after Wave 20's source/native unwind corpus.

**Ownership.** Edit only `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**`, and `crates/cgx2c/tests/memory_release_*`. Do not edit compiler production code, foreign adapters, notebook, root Cargo files, or shared docs. The record lane owns the other `cgx2c` files.

## Deliverable

1. Add a deterministic replay corpus that creates two or more auto/manual regions, owned and managed byte objects, duplicate roots or aliases, explicit legal copy/transfer, attempted illegal cross-region edge/escape, and close in different orders. For each operation, record region ID, handle generation, owner/root count, committed/live bytes, collection count, first error, and expected validity of surviving handles. A failed transfer must not leak a root, charge the destination, or invalidate the original. A successful copy must not alias mutable storage across regions.
2. Run representable cases through source `cgx2 run` and `cgx2c` debug/release output, including one legal copy/transfer and one statically rejected escape. Keep direct-runtime-only graph operations separately labeled. Compare defined output/diagnostics and root/region reports; do not invent a source operation to fill a coverage gap. Add minimal replay fixtures under the owned `memory_release_*` test prefix.
3. Inject failures before destination reserve, after source validation, and during region close where the runtime API exposes those points. Include nested `no_gc` around a cross-region copy: collection count must stay zero on success and failure, and both region and guard state must survive an error. Preserve first-failure reporting and idempotent cleanup.
4. Run an available dynamic tool over the smallest failing or highest-risk source/native path, with exact command, toolchain, instrumentation boundary, leak setting, and result in an owned report. If instrumentation cannot reach a boundary, record that gap. A finite corpus or zero root census is not a general safe-memory proof.

## Verification and handoff

Run owned runtime/conformance tests twice, targeted `cgx2c --test memory_release_*` debug/release cases, formatter, and owned Clippy `-D warnings`. Commit only owned paths; report SHA, exact replays, commands/results, source versus direct-runtime coverage, and remaining foreign/parallel/general-memory gates. Stop for review.
