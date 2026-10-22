# CGx2 Ownership and Region Analysis Core Implementation Plan

> **For agentic workers:** Implement this plan task by task in your attached worktree. Do not delegate or edit another lane. Commit the bounded deliverable and stop for user review.

**Goal:** Reject use-after-move, conflicting borrows, unsafe region close, and collection inside `no_gc` over a small explicit operation IR.

**Architecture:** A new `cgx2-ownership` library checks a source-spanned operation sequence. It models owned values, managed handles with region identities, borrow tokens, and `no_gc` nesting. Branches are analyzed conservatively at joins. A future compiler pass will lower `.cg` source into this IR after memory syntax is settled.

**Tech Stack:** Rust 1.98, standard library, `cgx2-syntax::Span` for source positions, no `unsafe`.

**Spec:** `docs/language-design.md` memory/ownership sections and `plans/wave3/README.md` source-independent seam.

## Global Constraints

- Own only `crates/cgx2-ownership/**` and this plan file. Do not edit parser, semantic checker, runtime, CLI, or root manifests.
- This is a static checker for an explicit IR, **not** a claim that all `.cg` programs are memory safe. Report unsupported loops and foreign aliases as outside the checked subset.
- A value has exactly one state: live owned, moved/dropped, or managed in an open region. `Move` consumes its source; `Use` after move/drop is an error.
- Shared borrows may coexist; a mutable borrow is exclusive. A value cannot be moved or dropped while borrowed. Borrow tokens must end before their enclosing branch exits.
- Closing a region is rejected while a live managed handle in it exists. `Promote` consumes an owned value and creates a managed handle in an open region; no implicit cross-region handle move is modeled.
- `Collect` inside any nested `no_gc` scope is rejected, regardless of target region. Allocations/budgets are runtime-enforced and not statically proved by this crate.

## Review Focus

1. Use after a move in only one branch must be rejected after the join; test in Task 3.
2. Mutable/shared borrow conflicts and move while borrowed must fail with the offending op's span; test in Task 2.
3. Closing one region must not invalidate a handle in another; test in Task 2.
4. Nested `no_gc` must reject `Collect` until all guards exit; test in Task 3.
5. An unknown value/borrow token or unbalanced branch state must be an explicit diagnostic, not a panic; test in Tasks 1 and 3.

## Files and public contract

- Create `crates/cgx2-ownership/Cargo.toml`, `src/lib.rs`, `src/ir.rs`, `src/check.rs`, `src/diagnostic.rs`, `tests/ownership.rs`, and `README.md`.
- Export `Op` variants `OpenRegion`, `CreateOwned`, `Use`, `Move`, `BorrowShared`, `BorrowMut`, `EndBorrow`, `Promote`, `CreateManaged`, `DropValue`, `CloseRegion`, `EnterNoGc`, `ExitNoGc`, `Collect`, and `If { then_ops, else_ops }`. Every variant carries a `Span`; use string identifiers for values, regions, and borrow tokens in this first slice.
- Export `check_ops(ops: &[Op]) -> Result<(), Vec<OwnershipDiagnostic>>`, where a diagnostic has stable `code`, `message`, and `span`.
- Codes: `CGO001` invalid/unknown/moved value, `CGO002` borrow conflict or leaked borrow, `CGO003` invalid region/promote/close, `CGO004` forbidden collection, `CGO005` unbalanced control state. Exact messages may evolve; codes and spans are the test contract.

### Task 1: Owned state and moves

- [ ] Add failing tests for create/use, move/use-after-move, double move, drop/use-after-drop, duplicate name, and unknown value with exact span.
- [ ] Run `cargo test -p cgx2-ownership --offline` and record failures.
- [ ] Implement the IR and linear state machine with deterministic diagnostics. Never silently revive a moved value.
- [ ] Re-run the tests and verify Task 1 cases pass.

### Task 2: Borrow and region rules

- [ ] Add failing tests for multiple shared borrows, exclusive mutable borrow, conflicting borrow, move/drop while borrowed, promote to open/closed region, close with live handle, and isolation between two regions.
- [ ] Run the tests red, implement borrow-token/region tracking, then rerun. Explicit `EndBorrow` releases only its own token.
- [ ] Verify managed values cannot be treated as owned or moved across regions implicitly.

### Task 3: Guards, branches, and handoff

- [ ] Add failing tests for nested `no_gc`/`Collect`, unmatched exit, branch-local move at join, borrow not ended in one branch, and a valid balanced branch.
- [ ] Run the tests red, implement branch analysis with a conservative join, then rerun all tests. If state cannot be joined safely, emit `CGO005`.
- [ ] Run `cargo test -p cgx2-ownership --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --check --manifest-path crates/cgx2-ownership/Cargo.toml`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-ownership --all-targets --offline -- -D warnings`.
- [ ] Document checked rules, branch assumptions, and missing parser lowering/loops/foreign aliasing. Commit only owned paths and stop.

**Acceptance:** finite operation programs get deterministic memory-rule diagnostics and safe cases pass. No source-level or whole-program memory-safety claim is made until compiler lowering and runtime checks are connected.
