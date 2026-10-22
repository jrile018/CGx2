# Trusted bounded source-contract proof

Use the attached worktree; read shared Wave 15 references and this plan. Commit only owned paths and stop for review.

**Goal.** Existing finite verification checks a bounded source subset and correctly reports global proof as unknown. Add one auditable solver-backed bounded proof path for checked `.cg` contracts, preserving exact i64 overflow behavior and never treating an unavailable solver or malformed output as proof.

**Ownership.** Only `crates/cgx2-verify-core/**` and `crates/cgx2-verify-cli/**`. Read frozen source-contract extraction API. Do not edit syntax/sema/CLI, package, root Cargo files, shared docs, plans or other lanes. An interface needed elsewhere is an integration note.

## Deliverable

1. Define one typed, bounded input fragment from existing checked `.cg` requires/ensures/assert operations. Emit canonical SMT-LIB for its integer/Boolean semantics, with explicit checked i64 range and overflow conditions, quantifier-free where possible. Include source span and assumptions in proof artifact; unsupported expressions remain `unknown` with reason.
2. Invoke an exact configured Z3 or CVC5 executable with time, memory and output bounds, no shell, deterministic options and version/hash recorded. Parse only a complete expected status/proof or model protocol. For `sat`, validate the witness through the existing evaluator; for `unsat`, ensure the query encodes a negated property plus all checked assumptions and no discarded obligations. If the solver cannot supply sufficiently trusted evidence under the current architecture, report `bounded_solver_result` and retain global status `unknown` rather than `proved`.
3. Test true, false, overflow, unsupported effect/foreign call, malformed solver output, timeout and absent solver. Use stub executables only to test protocol rejection/recording; a real solver result requires an installed solver and independent finite cross-check on small domains.
4. Update owned README/CLI help with exact command, solver selection/version, result taxonomy, proof trust boundary and excluded language forms. Keep previous finite verifier behavior and JSON diagnostics.

## Verification

Run offline tests for owned crates, format and targeted Clippy with warnings denied, finite differential cases, and conditional Z3/CVC5 run if installed. Report solver identity, exact formulas/properties, unsupported cases, skipped external gate and commit SHA.

**Acceptance.** A verified narrow source-contract result is auditable, and missing or untrusted solver evidence remains unknown rather than a false proof claim.
