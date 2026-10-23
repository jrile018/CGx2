# Source and native memory evidence

**Goal.** Memory correctness is the highest priority. Extend the Wave 19 direct-runtime unwind corpus into executable `.cg` interpreter/native cases and state precisely which first-release safety claims the evidence supports.

**Ownership.** Edit only `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**`, and new or existing `crates/cgx2c/tests/memory_release_*` files. Do not edit other compiler production paths, foreign adapters, notebook crates, shared docs, or root Cargo files. If a required compiler defect lies outside ownership, record a minimal replay and stop at that boundary for coordinator integration.

## Deliverable

1. Add deterministic, bounded `.cg` memory-function scenarios for normal return, early return, nested `no_gc`, reserve failure, explicit root duplication/release, region close, and a caught host panic where the source/native interface permits one. For cases that cannot be represented in `.cg`, keep the direct-runtime case clearly separate. Include exact seed, operation sequence, expected value or diagnostic, and expected root/region census.
2. Run each representable case through `cgx2 run` and `cgx2c` debug and release output. Compare exit class, output, first failure, and root census where exposed. A failing native artifact must not be labeled a source parity pass. Keep cancellation/panic cleanup idempotent and preserve the first error; fix only defects inside owned runtime/conformance files.
3. Exercise at least one available dynamic memory tool against source/native generated execution, not only the standalone runtime. Record command, compiler/toolchain, instrumented boundary, sanitizer settings, result, and exclusions in an owned evidence file. If the host tool cannot instrument generated artifacts, record the exact failed attempt and leave the corresponding release claim open.
4. Produce an owned release-gate checklist mapping each memory claim to direct-runtime, source/interpreter, native debug/release, foreign, and parallel evidence. Do not turn passing corpus tests into a general memory safety proof. Keep the corpus bounded for ordinary CI; place longer stress cases behind an explicit ignored flag.

## Verification and handoff

Run owned package tests twice, targeted `cgx2c` memory release tests in debug/release, `cargo fmt --all --check`, and Clippy for owned packages with `-D warnings`. Attach a shortest replay to each discovered failure. Commit only owned paths; report SHA, commands/results, dynamic-tool scope, files, and remaining source/native/foreign/parallel safety gates. Stop for review.
