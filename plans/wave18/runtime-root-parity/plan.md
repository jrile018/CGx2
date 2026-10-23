# Runtime root parity in source and native failure reports

**Goal.** Replace the misleading implication that clearing source bindings proves runtime root cleanup. Consume Wave 17's actual `Session::root_census()` in both interpreter and generated native memory paths. This is the highest-priority Wave 18 integration.

**Ownership.** Edit only `crates/cgx2-cli/**`, `crates/cgx2-emit-rust/**`, and `crates/cgx2c/**`. Read, but do not edit, `crates/cgx2-runtime/**` or `crates/cgx2-sema/**`. Do not change the root Cargo manifest/lock, shared docs, notebook, benchmark, proof, or foreign crates.

## Deliverable

1. Add explicit runtime-root data to operation-failure and final-unwind snapshots: `runtime_rooted_handles` and `runtime_root_registrations` from `Session::root_census()`. Keep existing source-observed `rooted_handles` and `rooted_edges` for schema compatibility. If census fails, report a named census error rather than silently substituting zero. Preserve primary execution error and source span.
2. In the interpreter, capture the operation snapshot before local binding unwinding, then capture final census after all normal cleanup attempts. In generated native output, call the same runtime API at equivalent points; never hard-code final runtime root zero. Check that cleanup failures remain visible. Keep output/report size and atomic report behavior already specified by the owned crates.
3. Add at least one source `.cg` fixture with duplicate roots or nested calls where source binding count and runtime registration count are meaningfully distinguished; include region close, collection, reserve-failure, and final-unwind cases. Compare interpreter with native debug and release builds on diagnostic code, span, runtime census, and semantic memory counters. Unsupported forms must still fail with source-located diagnostics.
4. Update owned README/API descriptions to state precisely that zero registered roots is necessary cleanup evidence, not comprehensive memory-safety proof. Do not add a new general-memory-safety or GPU claim.

## Verification and handoff

Run targeted offline tests for the three owned crates, native debug/release parity tests, `cargo fmt --all --check`, and Clippy on owned packages with `-D warnings`. Run a direct `cgx2 memory failure-report` and `cgx2c` smoke using a committed fixture. If sandbox or toolchain prevents a command, give exact command and reason. Commit only owned paths. Report commit SHA, changed files, commands/results, schema compatibility, and remaining census limitations; stop for review.
