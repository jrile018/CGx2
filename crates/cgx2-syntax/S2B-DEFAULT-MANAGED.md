# Default managed checkpoint, version 1

Contract frozen before implementation on 2026-10-24. This is a bounded
R03/R04 checkpoint. The acceptance report in `default-managed-evidence/`
must distinguish implemented rows from gaps; this document grants no release
acceptance.

Ordinary records, variants, List, Array and String use the default Session
graph. Scalars and immutable handles are evaluator values; payload bytes and
child edges in Session are authoritative. Immutable construction and persistent
updates preserve previous values. The existing 65,536-element, 1 MiB UTF-8,
65,537 expanded-node, depth and execution bounds remain checked. Chunked graph
storage preserves the runtime's per-object limits. Default region remains auto,
1 MiB trigger and 64 MiB limit. Graph traversal uses visited handle identities;
public materialization retains its independent expansion limit.

The additive source boundary is `managed.Ref<T>`, `managed.ref(value)`,
`managed.get(reference)`, `managed.set(reference, value)` and
`managed.collect()`. Only Ref indirection permits recursive types. Cells have
typed edges and Session identity. Ref-bearing public snapshots and Eq/Show
constraints reject before execution. Internal returned references transfer roots.
User declarations retain nominal identity; ambiguous managed builtin spellings
reject explicitly. No new operation is routed by a `memory.*` text marker.

`managed.try_alloc(EXPR)` is a checked special form. Its protected expression,
including nested allocation and root registration, executes inside recovery.
Only AllocationError is caught: BudgetExceeded, RegionLimit, HostAllocation,
InvalidRegion and ShapeLimit. Ok and Err are allocation-free intrinsic carriers.
Earlier mutations and successful charges remain committed. Affine capture is
rejected until failed-owner transfer is implemented. Session/guard entry failure
is uncaught and source-located. Explicit collection rejects transitively under
no_gc. Automatic collection is statically permitted but suppressed by the real
Session guard. Root/guard cleanup never collects.

Two effect dimensions are authenticated. Existing behavioral effects() remains
valid for previously pure immutable construction. MemoryEffectsV1 reports
may_allocate_managed, may_auto_collect_at_allocation,
may_explicitly_collect and may_allocate_root_bookkeeping. Explicit ref and
try_alloc require ManagedAlloc; set requires Mutate; get requires ReadManaged;
collect requires Collect and MemoryCollect. Unknown transitive summaries never
authorize a guarded call. Mutable reads/writes are not pure contract operations.

The sole source producer authenticates concrete types/layouts, source and body
identity, root/safepoint maps, both effects and complete callee/import closure.
Existing public exhaustive syntax/value/ownership v1/v2 enums retain meaning.
Managed aliases never masquerade as unique ownership analyzer inputs. Schema1
owned copied initializers keep inline semantics. Mixed execution must use one
Session; a second unguarded Session is forbidden.

Interpreter, native debug/release and unrelated-directory standalone Rust consume
the same authenticated representation. Each live parameter/local/argument/child,
match payload and returned handle is rooted before allocating safepoints. Roots
end promptly with temporaries and lexical bindings, with no transfer collection
gap. ScopedRoots::finish preserves the primary result and reports cleanup errors
separately. Public output is materialized while rooted.

Portable Python retains immutable application values. Explicit cells, collection,
guards and owned observables reject before artifact replacement. Unsupported
notebook/proof/advisory consumers reject before old walkers. Versioned API,
effect and memory output changes require explicit compatibility evidence.

Named regions, transfers, full guard/region recovery, broader ownership and
foreign/cancellation integration, leak-enabled audit, S3/S4 and all R01–R15 gates
remain mandatory. See the evidence report for actual implementation and checks.

## Mixed owned/managed bridge

The bounded coexistence boundary is a local function call with copied i64, Bool
or Unit parameters and result. An owned entry/caller retains the frozen v2
source adapter, inline copied initializers and affine CFG proof. Its managed
helper is independently checked by the same managed producer. Both execute
against the caller's borrowed Session, including active nested budgets. Managed
handles and owners never cross this bridge. Managed helper closures cannot call
back into an owned function. The authenticated mixed envelope binds both models,
actual helper effects, the bridge signature and original source. Direct affine
capture into managed.try_alloc is rejected. Broader mixed ownership signatures
and transfers remain part of the expressly retained later ownership gates.

## Additive implementation and compatibility notes

The checkpoint implementation and its limits are recorded in
`default-managed-evidence/README.md` and `summary.json`. Native Rust embeds the
same runtime evaluator and authenticated concrete model used by the interpreter;
it does not reproduce the old host aggregate representation. The closed version-1
wire codec is an emission-size optimization above 2,048 expressions and is
round-trip checked by the producer. Graph admission counts unique handle
identities (and scalar nodes); public snapshot expansion has a separate bound.

The copied-scalar mixed bridge also admits ordinary heap helpers without an
explicit managed marker. Mixed package ownership and mixed ReadArgs reject
pending their later gates. The new concrete memory index labels absent summaries
unknown and does not expose a synthetic managed main as the real owned entry.

Python UTF-8 work now has a separate cumulative 256 MiB counter; node/step work
remains bounded at 1,000,000. Source-check escape portability is deferred to the
existing target preflights, preserving CGR009/CGRS003/CGE003 runtime/Rust/Python
codes and artifact preservation. Foreign-only legacy package admission remains
unchanged; new managed observables combined with foreign execution are gated.
Ordinary memory reports now reflect the actual Session. Existing API schemas,
v1/v2 affine models and scalar-only owned initializers retain their contracts.
