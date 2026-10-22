# Trusted bounded source-contract proof

**Goal.** Carry forward the uncommitted Wave 15 proof deliverable. The existing finite checker can find bounded counterexamples but does not justify a global `proved` claim. Deliver one reviewable solver-backed fragment or a precise `unknown` when trust prerequisites cannot be met.

**Ownership.** Only `crates/cgx2-verify-core/**` and `crates/cgx2-verify-cli/**`. Read the frozen source-contract extraction API. Do not edit syntax/sema/CLI, root Cargo files, shared docs, or other lanes. No dependence on uncommitted Wave 15 proof worktree changes.

## Deliverable

1. Specify one typed checked `.cg` fragment of `requires`, `ensures`, and `assert` with exact i64 overflow behavior. Emit canonical quantifier-free SMT-LIB where possible. The artifact must include source spans, bound, assumptions, solver input hash, and unsupported obligations. Unsupported effects or foreign calls remain `unknown`.
2. Invoke an exact configured Z3/CVC5 binary directly with bounded time, memory, and output, deterministic options, and recorded version/hash. Reject missing, malformed, partial, or inconsistent responses. Validate `sat` witnesses through the existing evaluator. Mark `unsat` as a bounded solver result; use `proved` only if the complete property and all obligations have a justified trust chain.
3. Test true/false/overflow/unsupported/timeout/absent-solver paths. Stub executables test protocol rejection only. A real solver result requires an installed solver and an independent finite cross-check on small domains; if unavailable, report this gate unrun.

## Verification and handoff

Run offline tests for both owned crates, formatter, targeted Clippy with warnings denied, protocol fixtures, and a live solver test only when installed. Report exact fragment, result taxonomy, trust boundary, skips, and commit ID. Stop for review.
