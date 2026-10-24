# Independent S2b-A producer plan review

Decision: ACCEPT for coordinator commitment and sole-producer dispatch. No remaining dispatch blocker. This is plan review, not implementation acceptance.

Reviewed draft SHA-256: 39dfcfac4ca97a66065ad5c6fca3191a9ee8a23fb522699e345d72ce71bb0473
Reviewer: /root/aggregate_plan_review

## Basis

Compared the draft against the original s2b-ordinary-memory plan, frozen release contracts, ordinary-owned contract and fixtures, language-design memory guarantees and validation-plan memory gates. No code edits, tests or builds.

The sole producer scope retains all original aggregate source families: construction, parameters/results, partial field/index moves, projected loans/reborrows, control flow, recursion, generics/packages, mixed Session execution and cleanup. Source copy_owned/promote remain dependent B; A cannot defer ordinary ownership families into B. No consumer fanout or S3 follows automatically.

Contextual owned/try_owned creates fresh owned storage without implicitly converting existing managed aggregates. Additive ownership v4 and sema aggregate_v1 preserve legacy exhaustive APIs. Immutable source/body closure authorizes execution; arbitrary analyzer success does not. Stable typed owner trees account payload, slots, links, initialized/drop metadata and capacity through Session before commitment. Detachment preserves storage identity; no second unaccounted authoritative host tree is allowed.

## Review corrections verified

- Required source fixtures declare ManagedAlloc and Mutate where needed.
- The index projection example uses a named remaining loan, avoiding unsupported postfix field syntax.
- Scalar loan reads distinguish copied contexts from loan-typed passing/return; exclusive forwarding preserves provenance and reborrow rules. Referent mutation cannot rebind immutable handles; shared scalar writes and immutable ordinary scalar rebinding reject.
- Existing lower.rs:651-674 supports this distinction: Owner writes require binding mutability; Loan writes require Exclusive independently of binding mutability.
- Bounds checks precede detachment/loan creation. Admitted runtime bounds errors pass check/build/emit then require runtime target parity and cleanup. Static rejection fixtures remain a distinct category. try_owned cannot disguise bounds errors as AllocationError; copied/managed Option indexing stays unchanged.
- CGR080 is compatible: crates/cgx2-sema/src/ordinary_owned/engine.rs:95 fail() already uses this general owned runtime failure code. New exact bounds message/span does not reassign an unrelated diagnostic.
- Historical unsupported-feature negatives remain raw immutable reports with separate reviewed applicability reconciliation. Unsafe ownership negatives cannot be reclassified to obtain a pass.

## Acceptance and limitations

The manifest covers actual interpreter/native debug/native release/independent emitted Rust, source/package/library boundaries, private and unused generic bodies, exact dependency UTF-8 source identity, one compound interacting application, real owner/root cleanup and reached storage/metadata failure sites. Workspace gates, state-machine/generated checks, source authentication, artifact preservation and original regressions remain required. Existing numerical limits are preserved; added specialization/layout/SCC limits are explicit.

Coordinator must commit the plan and record its exact starting base before dispatch. Worker freezes concrete encoding, charge formulas and signatures in its owned contract before implementation. Tree detachment, projected joins and bounded recursive convergence remain implementation risks, not grounds to narrow required positive cases. R01-R15 and genuine external acceptance remain open under the release ledger.
