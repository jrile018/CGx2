# Measured parallel checked zip map

**Goal.** Extend CGx2's reviewed pure CPU execution primitive to two input vectors while preserving checked arithmetic, output order, and deterministic first-error behavior. Do not imply arbitrary `.cg` automatic parallelization.

**Ownership.** Edit only `crates/cgx2-execution-policy/**`. Do not edit parser, sema, CLI, runtime, LibTorch, root Cargo files, or shared docs.

## Deliverable

1. Add a pure operation equivalent to `left[i] * right[i] + addend` over equal-length `i64` slices. Sequential and scoped parallel CPU implementations must produce identical ordered results or the same lowest overflow index and error class. Reject length mismatch and infeasible worker counts before dispatch. No CGx2 managed `Session` handle or effectful callback may cross workers.
2. Extend the measured `Policy::Auto` machinery with a distinct workload/schema identity for this zip map. With no valid local profile, choose sequential. Permit parallel only after checked paired calibration/holdout samples on this exact operation and machine show a conservative end-to-end benefit, including allocation, planning, thread launch, and join. Expired, mismatched, tampered, or noisy profiles fail closed to sequential. Explicit GPU requests still report unsupported.
3. Add a bounded randomized correctness corpus across empty/unequal/small/large inputs, signed boundaries, multiple simultaneous overflows, worker counts, and profile failures. Include one repeatable calibration example; report actual times and uncertainty from this host if feasible without claiming universal optimality.
4. Update owned docs/API to distinguish this standalone primitive from source lowering, general parallel compiler analysis, and GPU support. Note that a future typed/effect/ownership IR must justify lowering `.cg` code into it.

## Verification and handoff

Run offline `cgx2-execution-policy` tests, release-mode example/calibration where feasible, formatter, and Clippy with `-D warnings`. Commit only owned paths and reproducible small fixtures, not machine-specific profiles as portable defaults. Report SHA, files, exact commands/results, chosen fallback behavior, and limits; stop for review.
