# Verify a checked `.cg` contract subset

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Accept a `.cg` source file in `cgx2-verify` and run the existing finite contract engine on a small checked pure function subset, with diagnostics tied to source spans. This is a bounded proof path, not a claim that a solver's UNSAT text proves a global theorem.

**Architecture:** Read the frozen Wave 11 parser and semantic checker APIs as dependencies; do not edit them. Extract a single selected public pure function with explicit `requires` and `ensures`, Boolean/`i64` parameters, supported arithmetic/comparisons, and an expression body if representable. Translate only nodes whose checked overflow/short-circuit semantics match `cgx2-verify-core` exactly. Require explicit finite domains via CLI arguments or an adjacent strict input file, record their exact bounds and source digest, and reuse `verify_bounded`. A function outside the subset returns a source-located unsupported/unknown result without inventing a contract.

**Ownership:** Only `crates/cgx2-verify-core/**` and `crates/cgx2-verify-cli/**`, including their crate-local manifests and tests. Do not edit syntax/sema/CLI, runtime, root manifests/lock, docs or plans. Coordinate no interface changes with the parallel source-native lane; consume frozen Wave 11 forms only.

## Required behavior

1. Add fixtures for true finite contracts, false postconditions with re-evaluated counterexamples, overflow, unsatisfiable or undefined preconditions, missing/too-large domains, multiple-function selection, unsupported calls/effects/foreign declarations, and a source change after a report. Preserve existing external JSON v1 contract command and result schema compatibility, or version an extended schema explicitly.
2. The output must distinguish `finite-domain proved`, `disproved with validated counterexample`, and `unknown`; `proof.kind` remains unknown for global claims without an independently checked proof. Solver observations remain untrusted protocol output. Include `.cg` path, function, byte span, source hash, parameter domains, max cases and exact engine status in machine-readable results.
3. A source file that fails the normal checker cannot be verified through a looser translator. No implicit foreign assumption, unchecked arithmetic, or unsupported loop is silently omitted. Tight input/case/time/output limits remain in force. If the frozen source APIs cannot expose a needed contract form without edits outside ownership, implement the largest sound subset and report the precise dependency.
4. Update owned READMEs with the accepted source syntax, command examples, status meanings and honest proof limits.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-verify-core -p cgx2-verify-cli --offline`, format and targeted Clippy `-D warnings`. Run one source success and one source failure CLI smoke. A real Z3/CVC5 check is conditional on installation and is not required to claim finite enumeration. Commit owned paths, report SHA, source subset, cases, and solver availability.

**Acceptance:** One checked `.cg` contract can be mapped into explicit finite verification without changing its semantics or inflating finite evidence into a global proof.
