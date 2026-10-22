# CGx2 ownership operation checker

`cgx2-ownership` checks a finite, explicit sequence of `Op` values. It is a static analysis core, not a parser or a source-level memory-safety guarantee. A future compiler pass must lower `.cg` source into this IR and preserve `cgx2_syntax::Span` locations. Every identifier is a `String`; value and region names are unique for the whole operation program. Borrow tokens must be unique while active.

Call `check_ops(&ops)` to receive `Ok(())` or a deterministic `Vec<OwnershipDiagnostic>`. Each diagnostic has a stable `code`, a human-readable `message`, and the offending operation's zero-based, half-open UTF-8 byte `span`. An unclosed borrow or `no_gc` guard points to its opening span; an incompatible `If` join points to the `If` span.

## Checked rules

- `CreateOwned { value }` creates one live owned value. `Move { from, to }` consumes a live, unborrowed owned source and creates a new owned destination. `DropValue { value }` consumes a live, unborrowed owned value or managed handle. `Use { value }` requires a definitely live value; it is also rejected during an active mutable borrow. Names cannot be reused after move or drop.
- `BorrowShared { value, token }` allows multiple shared tokens. `BorrowMut { value, token }` requires no other token for that value. `EndBorrow { token }` ends only that token. Moving, promoting, or dropping a borrowed value is rejected. Active tokens at program end or created inside a branch and not ended before branch exit are reported.
- `OpenRegion { region }` opens a named region. `CreateManaged { value, region }` creates a handle only in an open region. `Promote { from, to, region }` consumes a live, unborrowed owned value and creates a managed handle in an open destination region. `CopyToOwned { from, to }` requires a definitely live managed source without an active mutable borrow, creates a separate owned destination, and leaves the source live. `CloseRegion { region }` requires an open region with no live or possibly live handles in it. Handles cannot be moved as owned values or implicitly transferred between regions. Closing one region does not affect handles in another.
- `EnterNoGc` and `ExitNoGc` nest. `Collect { region }` is rejected with `CGO004` while any guard is active, regardless of region; outside a guard it requires an open region. Unmatched exits and guards left open at program end are reported.
- `If { then_ops, else_ops }` analyzes both arms from the same incoming state. A value live in only one arm becomes *possibly live* at the join: later use, move, and drop are rejected, while a possibly live managed handle still blocks region close. Borrow tokens created within an arm must end in that arm. Incompatible region, borrow, or guard states produce `CGO005`. Both arms may make the same state transition.
- `Unsupported { feature, span }` lets a future lowering pass report constructs this checker cannot model, such as loops and foreign aliases, as `CGO005` instead of silently accepting them.

Diagnostic codes: `CGO001` invalid, unknown, or moved value; `CGO002` borrow conflict or leak; `CGO003` invalid region, promotion, or close; `CGO004` collection forbidden by `no_gc`; `CGO005` unbalanced or unsupported control state.

## Scope

This crate does not parse `.cg`, prove allocation budgets, track byte counts, execute collection, model loops, or reason about foreign memory and aliases. `no_gc` allocation limits and region capacity are enforced by the runtime. An operation program that omits relevant source behavior cannot establish safety of the source program. The `Unsupported` operation should be emitted for every unmodeled operation until a sound lowering exists.

Run from the repository root:

```sh
cargo test -p cgx2-ownership --offline
RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --check --manifest-path crates/cgx2-ownership/Cargo.toml
RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-ownership --all-targets --offline -- -D warnings
```
