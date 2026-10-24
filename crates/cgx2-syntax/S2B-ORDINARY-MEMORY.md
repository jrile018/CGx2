# Ordinary memory, version 1

**Status: incomplete implementation.** The requirements below are normative.
The current producer does not complete the assigned ordinary-memory milestone.
See the implementation checkpoint and evidence ledger at the end.

This additive producer preserves the reviewed foundation, ordinary-owned v1/v2,
and default-managed interfaces. The governing acceptance contract is
`plans/release-completion/s2b-ordinary-memory/plan.md`; passing a narrower adapter
or rejecting a required source operation does not close that contract.

The implementation uses one runtime Session. Region names are generative lexical
identities, not values or user-implementable constraints. `memory.Default` denotes
the existing default Auto region (1 MiB trigger, 64 MiB limit). A named region
requires a limit; Auto uses a positive trigger, defaulting to 1,048,576 bytes.
Manual regions accept no trigger and never collect at scope exit.

`memory.try_region`, `memory.try_no_gc`, and `memory.try_session` establish affine
capabilities only in the Ok arm of a direct exhaustive match. The Session form
is the entire entry body; its Err arm runs without a Session and admits only
authenticated copied scalar operations. Entry contracts on that main reject.
Capabilities cannot escape, be stored, copied, explicitly dropped, or fabricated.

`memory.Value<R,T>` qualifies every heap-bearing part of a graph. Scalar
qualifications normalize away, repeated identical qualifications normalize
idempotently, and incompatible nontrivial brands reject. All fresh constructors
inside delayed `memory.try_in(r, EXPR)` use r; ordinary callee allocations keep
their declared provenance. Heap-bearing projections and synthetic aggregates
retain R. Ref writes preserve the brand invariant.

`memory.copy_to` preserves aliases and cycles while making independent storage.
`memory.promote` consumes an owner on either branch and returns the same owner
through `memory.TransferFailure<T>` on failure. Publication is transactional:
prepared nodes and roots are removed on failure without collection or refunds
of successful reservations. `memory.copy_owned` copies rather than stealing
managed storage; Ref-bearing owned conversions reject until an owned mutable
topology is specified. Recovery carriers do not allocate a CGx2 heap Result.

Owned aggregates use checked static projections and deterministic remainder
cleanup. Dynamic-index moves reject conservatively. Loans retain their origins
through projections, calls and reborrows; returned loans require surviving
borrowed inputs. Analyzer joins intersect definite liveness and union possible
liveness, moved paths, origins and loans. Both Result arms have explicit owner
states. Recursive summaries require a bounded fixed point and authenticated
bodies, including private and unused functions.

The producer authenticates original source or a checked package source bundle,
concrete layouts, substitutions, region/loan identities, ownership states,
effects, root maps and transitive callee summaries before execution or emission.
AST-only construction never claims source authentication. Native and standalone
Rust use the same runtime engine. Python and static consumers reject unsupported
semantics before replacing artifacts or reporting a successful proof.

Evidence must distinguish implemented source behavior, direct runtime controls,
synthetic reserve failures, real budgets, and external unrun gates. Host OOM in
compiler/evaluator bookkeeping, process abort and external kill do not acquire a
recovery guarantee. Leak-enabled assurance and independent audit remain separate.

## Implementation checkpoint

Implemented source paths currently cover generative lexical region entry,
region-generic helpers with explicit `shared<memory.Region<R>>` inputs and
`memory.Value<R,T>` results, delayed contextual allocation, synthetic collection
results retaining their region, managed-to-managed graph copying, recoverable
guard establishment, and a copied-scalar Session-entry fallback. The interpreter,
native debug/release and standalone Rust use one shared runtime engine. A failed
Session entry returns `ExecutionV1 { evidence: None, memory: None, .. }`; the CLI
and emitted program display `metrics: null` rather than fabricated zero counters.

The direct-runtime `OwnedGraphV1` is a sealed, independently allocated
`OwnedBytes` image with indexed edges, bounded to 65,537 raw objects, 64 MiB of
encoded storage and 1,000,000 traversed edges. `ScopedRoots::copy_graph_to_v1`
preserves graph aliases/cycles. `promote_graph_v1` reserves publication capacity
before committing and returns `PromotionFailureV1 { error, owner }` with the same
owner allocation on failure. Prepared destination objects and protected roots
are removed on failure without collection or budget refund. Traversal tables are
fallibly reserved and charged as logical runtime metadata; the accounting is not
allocator RSS. Raw image topology is a runtime mechanism, not permission for a
source-level Ref-bearing managed-to-owned conversion.

The v3 analyzer lowers opaque two-outcome transfer carriers into explicit CFG
edge blocks and delegates bounded liveness/provenance analysis to v2. It is not
yet the source producer's general aggregate ownership lowering. `MemoryEffectsV2` is inferred for each concrete function, including contracts,
private/unused bodies and recursive SCCs. A monotone reverse-call worklist
propagates every resource bit. The interface adds `may_create_region` and
`may_allocate_owned`; the prototype `may_enter_region` remains an equal alias.
Owned allocation is false for the currently admitted closed source subset;
unsupported ownership never receives an accepted summary. Missing generic
specializations remain unknown, rather than receiving a false summary.

`compile_ast` authenticates the supplied tree and reports `ast-only-v1` with no
original-source hash. AST-only input rejects template identifiers, non-source
region binder names and duplicate expression spans within a function, so caller
metadata cannot impersonate compiler capabilities or alias checked types.
Lexical region brands include the function identity and full scrutinee range.
`compile_source_bundle_v1` reparses original UTF-8 inputs,
checks direct-import visibility and every dependency body before merging, then
checks region provenance over the merged program. Operation identity is the
unspellable injected callee symbol, not a byte-span lookup; equal spans in
different package files cannot alias intrinsic operations. The additive public
`ApiMemoryIndexV2` exposes input/analysis identity, original declaration syntax,
region quantification, concrete types and per-function memory effects alongside
behavioral effects/capabilities. Managed `ApiMemoryIndexV1` retains its schema.

`cgx2_package::CheckedMemoryPackageV1` is distinct from `CheckedPackage`. It uses
the existing bounded manifest/source reader and checks canonical targets and
exact bytes before returning a program and before compiler publication. The
CLI supports checking/running these packages, Rust emission/native builds,
API metadata and artifact-preserving CGE006 Python rejection. Static dependency
diagnostics currently use the entry source path; runtime errors retain their
callee file and UTF-8 span. Exact static dependency-file attribution remains an
open acceptance row.

**Still unimplemented and required:** general aggregate ownership (including
partial moves, projected loans/reborrows, recursion and packages); source
`memory.copy_owned` and `memory.promote` with readable/retryable failure owners;
complete v3 body/summary authentication and aggregate ownership package
integration; exact static dependency-file diagnostics; the complete required source/package fault,
cleanup and parity matrix. Rejection of these operations is an incomplete row,
not implementation acceptance. External leak-enabled checks, independent audit,
performance and all broader release requirements remain open.
