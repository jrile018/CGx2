# Conditional memory profile

Memory scripts and acyclic local `Unit` memory functions admit the existing
mandatory-`else` statement, nested arbitrarily within this bounded profile,
including inside `no_gc`. Scalar/record APIs and syntax are unchanged.

A guard is a Boolean literal or one `==`, `!=`, `<`, `<=`, `>`, `>=` comparison.
Each of its two operands must be an in-range `i64` literal or a declared `i64`
function parameter. The parsed internal `$cgx2_cmp_*` call must have exactly two
positional arguments. Arithmetic operands, chained comparisons, arbitrary calls,
foreign/memory operations, truthiness, and Boolean parameters are rejected.
The selected guard is evaluated once. Both possible arms are checked statically,
even for literal guards.

An arm may operate on existing handles, regions, objects and checked reference
slots using the existing intrinsics, or call a local memory function. Local calls
retain isolated scopes and only accept the existing explicit `i64` arguments.
Call discovery, recursion checks, direct/transitive collection analysis, and
region-close checks inspect nested arms. Calls nested inside `no_gc` retain that
context, including calls in an inactive arm. Region closure in a function must
occur on every possible successful path.

Declarations inside an arm are restricted to owned/managed byte temporaries and
borrow tokens: `owned_bytes`, `alloc_bytes`, `move`, `promote_owned`,
`copy_to_owned`, `borrow`, and `borrow_mut`. Live byte temporaries must be
explicitly dropped and tokens explicitly ended before their arm joins. Moving
or promoting a temporary transfers that obligation to its destination. Dropping
an ancestor temporary in both arms of a nested conditional is valid. Implicit
cleanup on error does not satisfy the normal-path requirement. Branch-local
regions, object/reference declarations, scalar bindings, mutation, returns and
loops remain unsupported. Existing duplicate/shadowing rules also apply to
ancestor bindings that have already been dropped; this does not add shadowing.

Checker, interpreter and native emission assign arm declarations internal names
`$arm{declaration_span_start}:{source_name}`. Each arm clones its incoming source
name map and restores it at the join, so sibling arms and subsequent outer code
may independently use the same local name. The checker emits the existing
ownership `Op::If`; it never linearizes arms or uses the runtime-selected path as
an ownership oracle. Possible-path unavailability remains unavailable; differing
region, borrow and guard states are rejected by the existing join.

Only the executed path allocates or explicitly collects. Active nested `no_gc`
budgets remain cumulative across regions and local calls; root removal does not
refund a budget. Allocation failure follows the existing collector safepoints and
commits no new object. Manual collection counts stay unchanged without an
executed `memory.collect`. Byte handles have zero graph edges; `NotObject` at
slot zero is recognized by interpreter/native failure observers as a byte buffer.

On error, the failing statement records its source/runtime root census and
memory report before arm and call cleanup. Arm cleanup removes only newly
created bindings, unroots managed bytes and releases owned buffers/tokens.
Earlier outer transitions remain in effect. The first diagnostic and original
span survive subsequent cleanup; unroot errors are reported separately. Zero
final root registrations do not mean that unreachable manual-region objects were
collected: their charges can remain until collection, close, or session disposal.
Runtime APIs and collector policy are unchanged.

Python still rejects this memory profile (`CGE001`). Notebook pure admission
rejects memory/effects before execution. Verification remains scalar-only.
Readable success, nested-call/`no_gc`, graph, budget, region-limit, negative-size
and failure fixtures are `crates/cgx2c/tests/fixtures/conditional_memory_*.cg`.
See the owned [Wave 26 evidence](../cgx2-memory-conformance/release-gate-wave26.md)
for exact commands and instrumentation limits. This finite profile is not a
whole-language safety or leak-freedom proof.
