# Reliable measured CPU parallel zip map

**Goal.** Make the standalone checked CPU zip-map primitive safe and predictable under failure, cancellation, and noisy performance measurements before exposing it in `.cg` source.

**Ownership.** Edit only `crates/cgx2-execution-policy/**`. Do not edit core source/compiler, runtime, notebook, benchmark harness, foreign adapter, shared docs, or root Cargo files.

## Deliverable

1. Define and test exact outcomes for mismatch lengths, an arithmetic overflow in any chunk, worker panic, requested cancellation, and output allocation failure where injection is available. Do not expose a partially initialized result. Preserve deterministic first-error selection and make worker cleanup/join behavior explicit. Check zero/single element and uneven chunk boundaries.
2. Keep sequential and parallel modes observationally equivalent for every supported pure checked operation, including failure class and index. Add a deterministic corpus with fixed seeds and replay commands. For `Auto`, retain conservative sequential fallback unless calibration supplies a statistically defensible crossover for the exact operation and host; noisy samples must not be called a speedup.
3. Measure repeated sequential and parallel runs with warmup, a named host/core count, input sizes, and uncertainty. Store raw timing evidence and analysis in owned files; separate measured throughput from correctness. Do not claim arbitrary `.cg` automatic parallelism, GPU support, or memory-safe parallel managed objects.
4. Keep public API and default policy compatible unless a failing test demonstrates a necessary change. State the exact interface a future `.cg` lowering would consume, without editing source compiler crates in this wave.

## Verification and handoff

Run `cargo test --offline -p cgx2-execution-policy` twice, formatter, owned Clippy `-D warnings`, and the owned benchmark command. Commit only owned paths; report SHA, raw results, failure replay, and whether `Auto` stays sequential on this host. Stop for review.
