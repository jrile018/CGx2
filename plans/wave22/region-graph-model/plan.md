# Region graph model and source unwind evidence

**Goal.** Reduce the highest-priority memory correctness risk with an independent bounded model of failures and cleanup. Wave 21's transfer replay is the baseline, not a general proof.

**Ownership.** Edit only `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**`, and `crates/cgx2c/tests/memory_release_*`. Do not edit compiler production code, foreign adapters, notebook, other `cgx2c` tests, root Cargo files, or shared docs.

## Deliverable and contract

1. Add a deterministic, seed-replayable abstract model for at least two auto/manual regions with unique handle generations, owned values, managed objects, duplicate roots, links, explicit copy, collection, and close. Track committed/live bytes, root counts, collection counts, first error, and surviving-handle validity after every step. Persist failing seeds and minimal operation traces. Compute expected state independently of the runtime under test.
2. Compare the model with the committed `Session` API under normal and `fault-injection` features. Add new transitions beyond Wave 21: failure at different reserve indices, a nested guard exhaustion followed by retry, and close after a prior failed cross-region link. A failed reserve, invalid handle, illegal edge, rooted close, or exhausted guard must leave the original and unrelated region valid. A successful copy has distinct mutable storage. Include a deliberately injected model mismatch to prove the oracle detects divergence.
3. Add representable `.cg` cases under the owned `memory_release_*` prefix for legal transfer, close, and rejected escape. Compare `cgx2 run` with `cgx2c` debug/release outputs, diagnostics, and region reports. Label graph-link cases as direct-runtime-only. Do not invent new source syntax or broaden the source profile.
4. Run an available dynamic tool on the smallest high-risk generated/native case. Record exact command, toolchain, instrumentation boundary, leak setting, and result in an owned report. If leak inspection remains blocked, keep leak freedom open. A finite corpus cannot close the full supported-profile memory gate.

## Verification and handoff

Run owned runtime and memory-conformance suites twice, `cargo test -p cgx2-runtime --features fault-injection --offline`, targeted `cgx2c` `memory_release_*` debug/release cases, `cargo fmt --all --check`, and owned Clippy `-D warnings`. Commit only owned paths; report SHA, seeds/trace paths, commands/results, source versus runtime coverage, skips, and remaining memory release blockers. Stop for review.
