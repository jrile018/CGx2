# Ordinary owned Result contract, version 2

Frozen before implementation, 2026-10-23. This is an additive bounded R03/R04
deliverable. Schema 1 and ownership `v1` APIs retain their meaning. Full ordinary
memory, the original S2b-memory plan, and R01–R15 remain mandatory.

## Source and checked boundary

`try_owned(record)` evaluates a copied local scalar-field record once and calls
the actual runtime allocator. Its type is exactly
`Result<owned<R>, AllocationError>`. Allocation failure returns an Err without a
new owner; copied input remains usable. `owned(record)` retains its uncaught
failure behavior. Result locals, positional parameters/results, nested calls,
if/early returns, and `Result.Ok(value)` / `Result.Err(error)` match patterns are
supported. Matching consumes the sum. Bindings exist only inside their selected
arm; Ok is affine and discarding it drops its buffer. Direct Result constructors
are excluded explicitly; propagation uses checked try_owned-produced sums.
`AllocationError` is copied and matches exactly BudgetExceeded, RegionLimit,
HostAllocation, InvalidRegion, ShapeLimit, all nullary. No source spelling
manufactures an allocation error. Only actual runtime errors reach source arms.
Main still returns a copied scalar/record or Unit, never an owned sum/error.

Ownership `v2` adds conditional storage, parameter/result/call/return modes,
try-create/move/drop and discriminant CFG terminators without changing v1 enums.
A conditional slot is an affine sum, not an unconditional owned allocation.
The worklist consumes it on both match edges and initializes its payload only
on the Ok edge. Err has no payload owner. Definite liveness intersects, possible
liveness/origins union at joins. Calls preserve potential Ok provenance from
exactly one conditional parameter or fresh allocation. Divergent provenance
rejects. All bodies (including unused ones) must pass complete bounded checking
before summaries become available; recursive SCCs remain excluded. Source
parameter remapping omits copied slots and includes owned, borrowed and sum
slots in signature order. Layout/source/body/effect/arm identity is retained in
the privately constructed OwnedProgramV2, rebuilt and compared before use.
V1 accessors remain available for schema-1 modules; version-2 accessors expose
complete new CFGs/summaries. A v1 accessor cannot authorize a version-2 body.

Limits inherit v1 defaults: 256 blocks/locals/summaries/arguments, 4096
operations, projection depth 16, 256 name bytes, 10,000 analysis steps, 65,536
cached state entries, 4 MiB source. Source lowering checks these bounds before
expansion; no discriminant-path enumeration is performed. Runtime keeps
1,000,000 steps and call depth 64. Scalar layouts retain schema-1 packing.

## Session, guards and observations

`no_gc(budget: NONNEGATIVE_INTEGER_LITERAL):` accepts lexical statement blocks.
It uses the same single runtime Session across nested blocks and all callees,
with `Session::try_new` and `Session::try_no_gc`. Every active nested budget
includes the runtime's actual successful allocation and guard-table charges;
dropping an owner never refunds a successful allocation. Guard/session entry
failure remains an uncaught CGR080 at the original block/function span. An inner
failed entry preserves the outer guard and original error during cleanup.
No source region, managed collection, cross-region transfer, reborrow, or hidden
collecting callee is admitted in this profile. This is bounded owned-only no_gc,
not acceptance of the full cross-region/managed no_gc requirement.

Additive runtime observations expose cumulative successful runtime charges and
active guard depth; engine evidence adds actual Result outcomes, allocation
failures, guard entry/exit/failure, and runtime collection counts. Payload bytes
are separate from OwnedBytes capacity + 32 logical header bytes and guard-table
capacity charges. Evaluator/compiler host containers and foreign allocations
are not reported as CGx2 managed storage or a complete process memory report.
Synthetic reserve hooks are labeled injection, never real host exhaustion.

## Targets and remaining requirements

Interpreter and emitted/native/standalone Rust share authenticated model/engine
and runtime sources. Cleanup covers selected arms, discarded/returned Ok,
early return, checked arithmetic and failed argument evaluation. Python rejects
valid observables with CGE006 before artifact replacement; invalid source has
static errors first. Package/API/notebook/verifier/advisory consumers reject
unsupported new bodies before older walkers; no clean report/proof is inferred.

Generic/recursive/package summaries; broader loops/matching/partial moves and
reborrows; arbitrary Result provenance; default managed records/variants/List/
Array/String, aliases/cycles/roots; named regions; owned/managed/cross-region
transfers with failed-owner preservation; recoverable guard/region entry;
hidden/transitive collection enforcement; full fault/cancel/unwind and consumer
closure; leak-enabled and independent audit gates remain required. Typed
foreign/package/tensor/execution, command closure, S3 and S4 remain required.
Evidence and exact commands will be added under `owned-result-evidence/`.

## Implementation detail and compatibility clarification

`OWNED_RESULT_SCHEMA_VERSION=2` is additive; `SCHEMA_VERSION=1` retains its
public constant value. `OwnedProgramV2::schema_version`, `ownership_functions_v2`,
`summaries_v2`, and `analysis_sha256` expose immutable new evidence. The analysis
digest binds exact UTF-8 source, resolved layouts, typed effects/guard bodies,
CFG arm initialization and full checked callee summaries under a v2 domain tag.
Old `ownership_functions` and `summaries` are empty for new conditional/guard
modules. Existing source modules still pass both the v2 checker and lossless v1
checker projection. Existing `Evidence::json()` remains exact schema 1;
`json_v2()` adds observations for new source modules. Execution/emission always
rebuild and compare every representation/digest; no getter authorizes execution.

Before expanding a schema-2 body, source admission bounds statements and
expressions to 4096 each; schema-1 copied bodies retain their prior bounds. Ownership local, block and operation limits are checked before each
IR insertion. All original v1 analysis budgets remain unchanged. Recursion,
loops and arbitrary sum/loan provenance remain explicit source rejections.

Guard-only legacy functions with explicit Unit results and effects declarations
retain their original admission path even without a memory.* call. Ownership
wrappers or unresolved try_owned select the authenticated new profile first.
An unsupported legacy return remains CGM001; this is rejection compatibility,
not evidence of a successful legacy guard entry. Ordinary copied-result no_gc
functions select schema 2, and package walkers reject them with CGT080 before
attempting copied-value traversal.


## Explicit API version boundary

`compile_source(&str) -> OwnedProgram`, its `model::Model`/Ty/ExpressionKind/
StatementKind enums, `Evidence` struct literal fields and `Execution` result
remain schema 1. The original exhaustive matches and evidence JSON still
compile; new shapes reject with CGT081 at this old entry. `compile_source_v2`
returns `OwnedProgramV2` with `model_v2::Model`, `EvidenceV2` and `ExecutionV2`.
There is no placeholder v1 model for a conditional owner or guard. The old
producer can authenticate and `upgrade_v2`; conversion back to schema 1 is
lossless only when every typed/CFG shape is representable.

The compiler/CLI select compile_source_v2, and the new Rust API is
`emit_owned_result_rust(&OwnedProgramV2)`. The existing
`emit_owned_rust(&OwnedProgram)` authenticates/upgrades schema-1 input. Both
compile the same engine and actual runtime. The common SourceSpan type remains
schema 1, so Failure retains its original public field type. Dedicated tests
compile exhaustive old enums and an exact old Evidence struct literal, compare
old/upgraded execution, and require the explicit v2 entry for new source.

A Result/AllocationError match in function-tail position lowers each selected
arm's last expression to a checked return, including nested tail matches.
Non-tail matches retain statement/block scope and ordinary conservative joins.
This adds no new runtime IR or arbitrary Result constructor/provenance rule.

Schema 2 additionally bounds admitted lexical control/block nesting and admitted
expression nesting to 32 before typed/CFG expansion. This keeps nested guards
and emitted Rust data within a shared source/target envelope. Function call depth
remains 64 at runtime; schema-1 copied bodies retain their original bounds.
