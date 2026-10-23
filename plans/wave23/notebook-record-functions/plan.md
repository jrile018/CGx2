# Local record-typed pure notebook functions

**Goal.** Extend Wave 22 local record cells to pure functions with immutable scalar record parameters/results. The kernel currently restricts function annotations through `scalar_type()` to `i64`, `Bool`, and `String`.

**Ownership.** Edit only the five `crates/cgx2-notebook-*` crates, tests, fixtures, and examples. Consume committed syntax/checker/CLI record semantics without changing them. Do not edit package, foreign, root Cargo files, or shared docs.

## Deliverable and contract

1. Accept local previously declared or same-cell declared nominal records with one to eight `i64`/`Bool` fields in pure function parameter/result annotations. Support identity, field projection, and constructing/returning the declared record. Preserve nominal distinction, declaration-order storage/display, source-order argument evaluation, checked arithmetic, and immutability. Do not admit nested, mutable, generic, managed, imported, or foreign-layout records.
2. Resolve types against definitions visible to the selected cell and preserve function scope. Type annotations are dependencies even if the body never constructs the record. Editing/deleting/replacing a referenced record must stale dependent function/call cells. Commit definitions, bindings, and pending values only after success; a failure preserves prior valid state.
3. Reuse bounded tagged worker records and retained-value validation. Keep exact i64/Bool values, Unicode identifiers, timeout/cancellation, step/depth/replay-byte budgets, and pure effect/capability rules. Continue rejecting imported-record signatures and effectful or contract-bearing functions with cell ID and source span.
4. Test distinct records, record parameters/results across cells, unused type dependencies, reordered constructors, shadowing, Unicode names, unknown and not-yet-visible types, wrong nominal type, edit/delete invalidation, selective rerun, save/reopen, Restart and Run All, clean sweep parity, and recovery after failure. Add a readable `.cgnb` example and exercise CLI/UI. Do not replay a prior effectful cell as a hidden dependency.

## Verification and handoff

Run all five notebook suites offline, loopback HTTP tests where binding is permitted, formatting, and owned Clippy `-D warnings`. Commit owned paths; report example, commands/results, stale/provenance behavior, resource bounds, and unsupported imported/effectful profiles. Stop for review.
