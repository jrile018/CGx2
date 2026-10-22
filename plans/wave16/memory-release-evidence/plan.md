# Memory release evidence

**Goal.** Turn the Wave 15 direct-runtime auto-collection model into a reproducible release-gate packet with sharp scope. Memory safety is the top priority. Preserve the current public runtime API for the compiler lane.

**Ownership.** Only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Do not edit the CLI, emitter, foreign adapters, root Cargo files, shared docs, or other lanes.

## Deliverable

1. Enumerate actual auto/manual thresholds and safepoints from the implementation. Add a reproducible corpus with fixed seeds and trace minimization for rooted graphs, cycles, cross-region operations, nested `no_gc`, allocation failures, and region close. Compare liveness, collection counters, live/peak bytes, and root counts after each operation against an independent model. Record corpus count and exact time/memory bound.
2. Exercise cancellation/error cleanup and fault injection at allocation, edge growth, promotion/copy, and collection boundaries. Check that failed operations do not silently collect in manual regions or inside `no_gc`, that prior valid roots survive, and that later valid operations work. Fix demonstrated defects with a failing trace first.
3. Provide an owned release-gate report separating direct runtime evidence from source/native, foreign, thread/concurrency, and sanitizer/Miri evidence. Run a compatible dynamic tool only if installed; otherwise mark it `unrun` with the exact reason. A finite corpus is never described as a proof of all safe programs.

## Verification and handoff

Run targeted offline tests, formatter and Clippy with warnings denied, and repeat the deterministic corpus twice. Give exact commands, seed range, observations, residual risks, and commit ID. Stop for review.
