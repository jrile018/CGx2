# CGx2 Unique Owned-Buffer Runtime Implementation Plan

> **For agentic workers:** Implement this plan task by task in your attached worktree. Do not delegate, launch another chat, or edit another lane. Track the steps below and stop after your committed deliverable.

**Goal:** Add explicit unique owned byte buffers and safe transitions to/from the existing managed-region kernel.

**Architecture:** `OwnedBytes` is a non-`Clone` Rust value with deterministic drop and ordinary Rust exclusive/mutable borrows. `Session` creates it, charges allocations to active `no_gc` budgets, promotes it into a chosen managed region by move, and copies managed bytes back into a new owned buffer. This is a runtime primitive, not yet the CGx2 source ownership checker.

**Tech Stack:** Rust 1.98, standard library, existing `cgx2-runtime` crate, no `unsafe`.

**Spec:** `docs/language-design.md` memory model and `plans/wave2/README.md` runtime boundary.

## Global Constraints

- Own only `crates/cgx2-runtime/**` and this plan file. Preserve the Wave 1 public API and its 20 passing tests.
- `OwnedBytes` has one owner, no `Clone` implementation, `as_slice()`, `as_mut_slice()`, and `len()`. It cannot expose a managed handle or become a second owner implicitly.
- `Session::alloc_owned_bytes(len)` creates a zero-filled buffer and charges the full reserved capacity plus a documented fixed owned-header charge to every active `no_gc` budget before success. Owned bytes are not in a managed region and are never collected.
- `Session::promote_owned(region, value)` moves the owned allocation into a managed slot after checking region headroom and active budgets. On failure, return the unchanged `OwnedBytes`; on success, source ownership is consumed and the returned `Handle` is region-bound.
- `Session::copy_managed_to_owned(handle)` makes a separate byte copy, preserves the source handle, charges active budgets, and returns an explicit allocation error through `MemoryError` if needed. Do not implement a purported unique steal from managed yet.
- Logical charges are separate from RSS; clarify whether a moved payload is charged once or again during promotion. Never claim the first-release memory safety guarantee without compiler escape checking.

## Review Focus

1. Failed promotion due to region limit must return the original bytes unchanged; test in Task 2.
2. Failed promotion due to nested `no_gc` budget must leave region stats and source ownership intact; test in Task 2.
3. Copying a managed object must keep the managed handle valid and provide nonaliasing bytes; test in Task 3.
4. Zero-length and huge requested allocations must return a defined result without integer overflow or panic; test in Task 1.
5. An owned allocation inside `no_gc` must charge every active budget and must not trigger collection; test in Task 3.

## Files and public contract

- Create `crates/cgx2-runtime/src/owned.rs` and `tests/owned.rs`; modify `src/lib.rs` and existing internal accounting modules only as needed; update crate `README.md`.
- Export `OwnedBytes`, `OWNED_METADATA_BYTES`, `Session::alloc_owned_bytes(&mut self, len: usize) -> Result<OwnedBytes, AllocationError>`, `Session::promote_owned(&mut self, region: RegionId, value: OwnedBytes) -> Result<Handle, (AllocationError, OwnedBytes)>`, and `Session::copy_managed_to_owned(&mut self, handle: Handle) -> Result<OwnedBytes, MemoryError>`.
- If an existing private object constructor needs to accept a moved buffer, keep that change inside the runtime. Preserve generation checks and all Wave 1 collection behavior.

### Task 1: Unique owned allocation

- [x] Add failing tests for zero-filled length, mutable then immutable borrow, unique move, zero length, huge failure, and an active-guard budget charge.
- [x] Run `cargo test -p cgx2-runtime --offline` and record the initial failure.
- [x] Implement `OwnedBytes` and `alloc_owned_bytes` with fallible reservations and checked charges; add a compile-fail doc test or equivalent evidence that `OwnedBytes` is not cloneable.
- [x] Run the tests and verify Task 1 cases pass.

### Task 2: Move into managed region

- [x] Add failing tests for successful promotion preserving bytes, consumed source, region-limit failure, invalid region, nested-budget failure, and no collection during guard.
- [x] Run the tests to observe failures, then implement `promote_owned` as an atomic move with the error returning the source intact.
- [x] Run the tests and verify region stats, roots, and later collection behavior remain correct.

### Task 3: Copy from managed and handoff

- [x] Add failing tests for nonaliasing copy, stale handle, active-budget failure, zero-length copy, and the Review Focus 5 nested guard case.
- [x] Run the tests to observe failures, implement `copy_managed_to_owned`, and re-run all runtime tests.
- [x] Run `cargo test -p cgx2-runtime --offline`, `cargo fmt --check --manifest-path crates/cgx2-runtime/Cargo.toml`, and `cargo clippy -p cgx2-runtime --all-targets --offline -- -D warnings` from the repository root (format and Clippy using installed Rust 1.98.1 because the active stable toolchain lacks those components).
- [x] Document exact accounting and limits, including that compiler borrow/escape rules and foreign memory accounting remain future work. Commit only owned paths and stop for review.

**Acceptance:** owned storage has single Rust ownership, promotion failures retain the source, copies do not alias, and every managed/owned allocation inside `no_gc` respects budgets. The crate remains safe Rust with no `unsafe`.
