# Function analysis v1: compiler handoff

`cgx2_ownership::v1::analyze_function(function, summaries, source, limits)`
checks an explicit ownership CFG. `AnalysisReport::is_accepted()` requires a
complete analysis with no diagnostics. A failure returns the deterministic first
error, with the offending operation/terminator span and related loan span when
applicable. No state after an error is usable. This API is additive: legacy `Op`,
`check_ops` and `OwnershipDiagnostic` remain unchanged.

This is a **library delivery**. The existing `.cg` compiler does not lower ordinary
application functions into this API. The source fixture and executable Rust
example hand-author typed ownership decisions to demonstrate the input and byte
locations; they are not source borrowing acceptance tests. `Buffer` in that
fixture is deliberately treated as uniquely owned by the example metadata; the
current source type system does not infer that policy from the record declaration.

## Lowering recipe for S2

1. Resolve/type-check the complete reachable package before ownership lowering.
   Give every function and local slot a unique resolved identity (the v1 strings
   are IDs, not unresolved source spelling). Parameters are the first local slots
   in signature order. Remaining slots start uninitialized. Preserve original
   source bytes; attach zero-based half-open UTF-8 byte spans to function, locals,
   operations, and terminators (`Block.span`). Imported summaries have no local
   source location; call failures point to the caller's span.
2. Assign each parameter `Owned`, `Shared`, or `Exclusive`. Supply a checked
   `FunctionSummary` for every call, including effects-hidden/foreign operations.
   This function's own summary is automatically registered; exclude it from the
   `summaries` slice. Recursion uses contracts, never recursive analysis expansion.
   Validate every local body against its declared summary, including recursive
   SCCs, before trusting callers. An accepted caller with an unchecked callee is
   not a safety certificate. Imported/package provenance and native artifact
   authentication remain compiler/SDK obligations.
3. Choose `ResultMode::Unit`, `Owned { from_parameter }`, or a `Loan` with exact
   parameter index, structural projection and kind. `Owned(Some(i))` means derived
   ownership (possibly a subobject) from owned argument i, with the same storage;
   `Owned(None)` promises an independent unregioned owner. A loan result must
   derive from a compatible borrowed parameter, never a callee-owned parameter or
   local. Shared inputs cannot yield exclusive results. An owned result cannot
   implicitly alias other arguments. Summaries must describe all observable
   ownership, not just the returned object's type.
4. Emit one `Block` per basic block with `Goto`, `Branch` or `Return`. Branch
   conditions are intentionally abstract: both edges are possible. Emit every
   ownership-relevant operation on every reachable path, including early exits,
   errors and cleanup. Nonreturning cycles are allowed; program termination is not
   proved. Every input block (even unreachable blocks) is validated, and any
   `Unsupported` operation rejects the input.
5. Represent aggregate paths with `Field`, `Tuple`, known `Index`, or
   `DynamicIndex`. Only a typed layout can establish that different field/index
   names denote disjoint subobjects. Reject unions, overlapping layouts, raw
   dereferences, aliasing managed graphs and reference-containing aggregate fields
   using `Unsupported`; **do not invent distinct local IDs for aliases**. This API
   does not validate field existence, bounds, types, or physical layout. Copyable
   scalar reads use `Read`; owned payloads require `Move`/`Drop`. `Write` mutates
   scalar/non-owning contents only; replacing owner/loan fields requires explicit
   consume/initialization operations or rejection. There is no implicit copy or
   cross-region alias.
6. Explicit `Borrow` creates a token; `UseLoan` accesses that token and `EndLoan`
   ends it. Direct access is forbidden while an overlapping exclusive loan lives;
   moves, drops and writes conflict with any overlapping loan. Call arguments are
   places with invocation-scoped shared/exclusive access or ownership transfer.
   Overlapping arguments are permitted only if both are shared. A loan result
   extends access to the selected argument projection until `EndLoan`. Token
   forwarding/reborrowing through existing exclusive tokens is outside v1: end
   the old loan where semantically valid, otherwise reject lowering. No automatic
   nonlexical lifetime inference or loan cancellation is implied.
7. `Move` consumes the selected path; siblings remain usable, but ancestors and
   moved descendants are unavailable. Paths with a dynamic index cannot be
   consumed. `DropRemainder` explicitly destroys a partially initialized aggregate
   once its loans end. A dead slot may be reinitialized, allowing loop-local
   allocations; overwriting a possibly live slot rejects. Unreturned local owners
   receive implicit scope cleanup at return, after every nonreturned loan ends.
   Runtime lowering must actually perform that cleanup on success/failure/unwind.
8. `Storage::Region(name)` represents a unique region-bound aggregate/handle,
   **not** arbitrary aliased managed memory. Region parameters refer to already
   open caller regions, which the callee cannot close. Local regions must open
   before initialization and close after all possibly live values and loans end.
   Moves/calls preserve storage exactly. Returning a local region owner or local
   borrow rejects. Explicit copy/region conversion, collection, `no_gc` accounting,
   managed graph tracing and runtime failure semantics need separate lowering/
   runtime checks; v1 does not subsume the legacy memory operation checker.
9. Foreign summaries must authenticate `CallOnly` or `UntilRelease { parameters }`;
   `Unknown` and missing summaries reject. Retention names distinct borrowed
   parameter indices, keeping shared/exclusive loans until `ReleaseForeign(owner)`.
   Calls must supply a distinct active `retention_owner` token. No retry/release
   assumptions may be inferred from a foreign error. Lower the real owner release
   only after it succeeds or runtime lifetime rules guarantee cleanup. Foreign
   retention cannot escape the analyzed function in v1; escaping foreign owners,
   callbacks, raw aliases and nullable/uncertain release require a future contract.
10. Accept only `report.is_accepted()`. Never convert budget failure, unsupported
    lowering, invalid spans or missing summaries into success. Run ownership
    checking before execution/emission and preserve the v1 diagnostic separately
    or explicitly adapt it into source diagnostics (no shared enum change needed).

## Joins, loops and budgets

The iterative worklist caches incoming states per block. Definite liveness and
open regions intersect; possible liveness, moved paths, origins, loans and open
regions union. A token present on only one predecessor is possibly active: it
still blocks conflicting access, but cannot be used or ended as definitely live.
Different loan kinds for one token reject conservatively. Partial moves union,
including zero-trip paths. A loop revisits the header/body until the finite state
stabilizes; a later iteration can reject a move or a carried loan. Projection
provenance growth and resource exhaustion fail closed. This replaces neither the
old checker nor its existing state-preserving `Op::Loop` behavior.

All `AnalysisLimits` fields must be positive. Defaults:

| Field | Default |
| --- | ---: |
| `max_blocks` | 256 |
| `max_operations` | 4096 |
| `max_locals` (also parameter/argument count) | 256 |
| `max_summaries` (including this function) | 256 |
| `max_projection_depth` | 16 |
| `max_name_bytes` | 256 |
| `max_steps` (operations, terminators, edge propagations) | 10000 |
| `max_state_entries` (all cached states plus working state) | 65536 |
| `max_source_bytes` | 4194304 |

State entries count roots, moved paths, origins, loans, loan targets, regions and
their projection components. The budget bounds analysis state, not exact allocator
bytes or wall-clock time. CFG/input validation also applies before traversal; all
sizes/names/projections are bounded. Successful analysis need not have a reachable
return. These are ownership safety checks, not SMT proofs or execution benchmarks.

| Code | Meaning |
| --- | --- |
| `CGO101` | unavailable/moved/partially initialized value or occupied destination |
| `CGO102` | loan/access conflict, illegal strengthening, or escaping active loan |
| `CGO103` | region/provenance/return escape violation |
| `CGO104` | unknown/missing foreign lifetime or invalid retention owner |
| `CGO105` | malformed input/contract/CFG/span or unsupported operation |
| `CGO106` | input/state/step limit; `complete=false`, never accepted |

Invalid spans reject at `0..0`; all other primary and related spans are validated
UTF-8 boundaries in `source`. Tests use a multibyte prefix to detect byte/character
confusion. Diagnostics are deterministic for the same input and limits.

## Executable fixtures and remaining acceptance

```sh
export CARGO_BUILD_JOBS=2 RUSTUP_TOOLCHAIN=1.98.1
export CARGO_TARGET_DIR="$PWD/target/release-ownership"
export TMPDIR="$PWD/target/tmp-ownership"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo run -p cgx2-ownership --example function_analysis --all-features --offline
cargo test -p cgx2-ownership --all-features --offline
cargo clippy -p cgx2-ownership --all-targets --all-features --offline -- -D warnings
cargo fmt --manifest-path crates/cgx2-ownership/Cargo.toml --check
git diff --check
```

`tests/function_analysis.rs` supplies positive and negative ownership inputs for
calls, exact parameter/result loans, partial moves/overlap, foreign retention,
CFG joins/loops, region escape and limits. The Rust example maps the two source
excerpts in `fixtures/call-spans.cg` to versioned inputs and checks the moved-source
diagnostic's actual byte slice.

S2 must replace hand-authored decisions with checked typed lowering, then run the
same accepted/rejected matrix through `cgx2 check`, interpreter and native output,
comparing diagnostics and runtime cleanup. S3 must cover notebook/foreign owner
escape contracts. This delivery cannot close those source/runtime gates. S4 leak-
enabled runtime/FFI assurance, independent memory audit and calibrated performance
evidence remain unrun here: invoke the repository's release evidence pipeline on
the integrated compiler/runtime revision with the required runner/artifacts. No
source borrowing, leak-freedom, solver, real foreign runtime or benchmark claim is
based on these in-process library fixtures.
