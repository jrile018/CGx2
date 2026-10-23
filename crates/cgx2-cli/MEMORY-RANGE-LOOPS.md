# Bounded memory range loops (Wave 27)

Memory scripts and acyclic local `Unit` memory functions accept the existing
`for i in range(start, end)` syntax. This is a bounded source/native preview,
not a whole-language memory-safety or leak-freedom result.

Endpoints are exactly two positional signed i64 literals, declared i64 function
parameters, or enclosing indices. They are evaluated once, left then right.
Ascending half-open ranges use signed comparisons without subtracting endpoints;
equal or descending endpoints do no work, including at i64 MIN/MAX. Endpoint
arithmetic, calls, Bool values, step arguments and alternate iterators reject.
Indices are immutable scoped i64 values usable in byte sizes, existing scalar
call arguments and exact comparison guards. Parameters, active indices and memory
bindings cannot be shadowed. Sibling loops can reuse an index name.

Each iteration can use outer handles/regions, byte temporaries, local borrows,
move/promote/copy operations on locals, explicit collection outside `no_gc`, and
isolated local function calls. Every temporary must be explicitly dropped or
consumed into another explicitly dropped local, and every local borrow ended on
all successful paths. Region/object/reference declarations and graph mutation
inside loops reject. Called functions retain their own checked region scopes;
only i64 arguments cross their boundary. Outer ownership, availability, regions,
borrows and active guards must remain unchanged. Returns, breaks, index mutation,
local escapes and general mutation remain unsupported.

`cgx2-ownership::Op::Loop { body, span }` checks a body from the incoming abstract
state and requires the backedge to equal that state, ignoring dead scoped-local
entries. The zero-trip join keeps the incoming state. It checks possible paths
even in empty loops and inactive conditional arms; there is no unrolling or
conditional approximation. The sema script and function paths use this operation.
Function extraction tracks lexical index scopes at call sites. Local-call,
collection and effect walkers recurse through loops. `closes_region` deliberately
never uses a possibly empty loop as evidence of closure.

The interpreter and Rust emitter restore iteration and conditional scopes on
success or failure, removing only values actually created. First failure spans
and pre-cleanup root snapshots survive nested arm/loop/call unwinding. Cleanup
errors remain secondary. An unchanged operation at a statement-step limit records
its own rollback observation before surrounding scope cleanup, matching native
reports instead of comparing an entire compound statement's work.

There is one non-configurable 100,000 entered-iteration counter per memory program
invocation, shared through all nesting, conditionals and calls. Empty ranges
consume none. The next attempted entry fails with `CGR026` and message
`memory entered-iteration limit of 100000 exceeded` at that loop's span. This is
independent of the existing 1,000,000 statement-step limit (`CGR010`). Loops are
lowered once; neither IR nor source expands per iteration. `no_gc` charges remain
cumulative across iterations, regions and calls. Drops do not refund charges or
collect manual objects; explicit collections execute only on selected paths.
No runtime collector policy or public runtime API changed.

Fixtures live in `../cgx2c/tests/fixtures/range_memory_*.cg`. Tests cover signed
boundaries, zero/one/multiple iterations, lexical nesting, sibling names,
conditional guards, local calls and borrows, static invalid paths, cumulative
exact/one-byte-short budgets, manual pressure versus collection, auto collection
before a reserve fault, failures after successful iterations, both limits and
pre-cleanup versus final root census. Compiler tests compare deterministic region
and root reports across interpreter/native debug/native release, excluding
allocator identities and pause timings. The reserve-fault sweep covers indices
1..=24 (13 reached reserves); later indices execute successfully.

Reproduction commands and results are in
`../cgx2-memory-conformance/release-gate-wave27.md` and
`../cgx2-memory-conformance/evidence/wave27-verification.json`. Python memory
emission and pure notebook execution still reject these programs. The scalar
verifier reports unknown/unsupported, with no solver invoked. Those consumer
crates were exercised read-only.

ASan instruments the runtime and generated debug/optimized binaries. Leak
detection is disabled: host-blocked LSan remains open. These checks do not cover
system allocator internals, foreign/Python/LibTorch memory, live solver trust,
delegated cgroups, paired human/model trials, or the whole-language release gate.
