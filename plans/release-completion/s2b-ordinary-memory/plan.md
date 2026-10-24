# S2b ordinary aggregate ownership, named regions, and recoverable entry

Dispatch prerequisite: coordinator acceptance of the combined default-managed repairs. This is the next connected implementation milestone of s2b-memory/plan.md and R03/R04, within the existing four-stage completion plan. One sole source/runtime producer owns this interface change. No consumer fanout or library-only stopping point. All R01–R15 requirements remain mandatory.

## Read first and exclusive ownership

Read AGENTS.md; docs/language-design.md and validation-plan.md; plans/roadmap.md; plans/wave28/{README,REVIEW}.md; release-completion/{README,contracts,worker-rules,dispatch}.md; s2b-memory/plan.md; s2b-default-managed-review/README.md and combined acceptance; existing S2B-FOUNDATION, S2B-ORDINARY-OWNED, S2B-OWNED-RESULT, S2B-DEFAULT-MANAGED and AUTH-REPAIR contracts; ownership ADAPTER; runtime graph/scope/owned contracts and memory-conformance/release-memory.md. Use MegaMemory overview/query/record and Serena initial instructions.

Sole full writer: crates/cgx2-{syntax,sema,package,api-index,cli,emit-rust,emit-python,runtime,ownership,memory-conformance}/** and crates/cgx2c/**. Mechanical exhaustive compatibility/conservative rejection only: notebook format/kernel/session/cli/ui, verify core/cli, effect-index, security. No concurrent consumer owns these paths. Root Cargo.toml/Cargo.lock, docs/plans/coordinator ledgers, bench/tools, foreign/bind/ffi/tensor/worker/execution/conformance adapters and historical evidence are read-only. New owned contract/evidence: crates/cgx2-syntax/S2B-ORDINARY-MEMORY.md and ordinary-memory-evidence/**. Existing contracts may gain clearly labeled additive version notes; preserve historical reports.

Work only in attached worktree on riley/ branch. No delegation/new chats/follow-on task, main merge/publication, packages/installations, external spending/contact, or security changes. Use existing offline dependencies; report exact root-lock reconciliation for coordinator without committing root-lock edits.

## Frozen source contract

Use existing calls, matches, qualified types and constrained generic syntax. Resolve builtin identities semantically, preserving user namespace/nominal precedence and legacy memory-script routing. A memory.* spelling or substring alone must not select the new profile. Publish exact ambiguity diagnostics and regression tests. No user-defined function/type may manufacture trusted region identities or memory capabilities.

### Region scopes and provenance

`memory.RegionName` is a compiler-only non-value generic binder kind, not a user-implementable constraint. `memory.try_region(mode: memory.Mode.Manual|Auto, limit: N, trigger: N)` is a scoped special form and must be the direct scrutinee of an exhaustive Result.Ok/Result.Err match. The limit is required and nonnegative. Auto-only trigger is positive and defaults to 1,048,576 bytes; Manual with a trigger rejects. Numeric arguments evaluate once, left to right. Invalid literal bounds reject statically; invalid dynamic bounds return AllocationError.ShapeLimit.

The Ok binding is an affine `memory.Region<R>` capability, with a fresh generative lexical region brand and a distinct runtime identity for every dynamic entry. Local inferred brands need no source spelling. Region capability requires no owned<> wrapper; `borrow(r)` returns `shared<memory.Region<R>>`. It cannot be copied, stored, returned, manufactured or explicitly dropped to bypass its scope. No exclusive capability is needed for ordinary region allocation; lifetime/closure authority belongs to the successful arm.

`memory.Value<R,T>` carries the provenance of its entire managed graph. The wrapper is admissible for managed-compatible T; it cannot hide owned values, loans or affine capabilities in a traced graph. Structural operations preserve R. Normalize Value<R,i64>, Value<R,Bool> and Value<R,Unit> to the corresponding graph-free copied scalar type, for every R. Normalize Value<R,Value<R,T>> idempotently; conflicting nested nontrivial brands reject. Projections, variant matching and indexed reads yield a scalar only when graph-free; heap-bearing subvalues retain R. Any synthetic Option or other heap aggregate produced by get/index/slice/updated/append and similar operations is allocated in R and has the outer Value<R,...> qualification. Never wrap an R handle in an ordinary/default heap Option/Result. Allocation-free intrinsic recovery Result carriers are the explicit exception and retain payload provenance. All generic/package signatures use this same normalization. Public/package helpers quantify region parameters explicitly and retain them in authenticated summaries:

```cg
fn build<R: memory.RegionName>(r: shared<memory.Region<R>>) -> Result<memory.Value<R, List<i64>>, AllocationError> effects(ManagedAlloc):
    memory.try_in(r, [20, 22])

fn work() -> i64 effects(ManagedAlloc):
    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):
        Result.Ok(r):
            match build(borrow(r)):
                Result.Ok(xs):
                    length(xs)
                Result.Err(error):
                    -2
        Result.Err(error):
            -1
```

`memory.Default` is the compiler-owned brand of the current Session default region. `memory.Value<memory.Default,T>` canonicalizes to existing ordinary managed T, preserving old source behavior. No cast, user declaration, generic instantiation or existential packaging can erase a nondefault brand. `memory.default_region()` yields a borrowed default capability; it does not allocate another Session. Default region remains Auto, trigger 1 MiB, limit 64 MiB.

The Ok arm owns a region scope. On normal exit, early return or failure, destroy dependent values/owners and release roots in defined reverse lifetime order before closing storage. Escaping handles/loans reject statically, including through generic nested fields, Result payloads, closures if admitted, and mutable Ref cells. Closing is deterministic release, never a collection or budget refund. This milestone uses lexical closure, with no explicit early-close operation. `memory.collect(borrow(r))` explicitly collects only R; no implicit cross-region edges or hidden manual collection.

### Contextual allocation and graph transfers

- `memory.try_in(r, EXPR)` is delayed evaluation inside an allocation recovery boundary, returning `Result<memory.Value<R,T>, AllocationError>`. Every fresh nested constructor in EXPR uses R. Callees retain their declared/authenticated allocation and result provenance; caller context must not silently relocate callee allocations. Existing child handles from another region reject unless explicitly copied. Check the final result as well: an already-R value is retained/rooted without implicit copying, a graph-free scalar returns its ordinary scalar type, and an existing default-region value or default-allocating callee result rejects unless explicitly copy_to'd into R. try_in is never a rebranding cast. Evaluate every expression once in source order. Catch allocation failures only; arithmetic/contracts/invariant failures keep their own semantics.
- `memory.copy_to(r, value)` deep-copies the reachable managed graph into R, preserving aliases/cycles via visited identities. Source stays valid. Destination must be closed over all reachable nodes, never a graph of cross-region raw handles.
- `memory.promote(r, owner)` consumes owned<T> and returns `Result<memory.Value<R,T>, memory.TransferFailure<T>>`. Success transfers ownership into R; failure contains `error: AllocationError` and the exact original `owner: owned<T>`. This promises ownership transfer, not zero-copy timing. The input binding is consumed on both arms; the failure carrier owns the single surviving owner. `failure.owner` may be moved out once; leftover fields/remainders have deterministic cleanup. Promotion while an input loan survives rejects.
- `memory.copy_owned(value)` makes an independent owned<T> copy without stealing shared managed storage. Immutable DAGs may expand to owned trees within existing bounded shape limits. Ref-bearing mutable/cyclic graphs are explicitly rejected by this conversion until a sound owned representation exists; do not raw-copy handles or call this topology implemented. Managed-to-managed copying must still preserve Ref aliases/cycles.

All intrinsic recovery carriers are allocation-free, including Ok managed values, region/guard/session scopes, and Err owner payloads. memory.TransferFailure<T> is an intrinsic affine carrier, not a user-constructible record or an ordinary heap Result. The frozen five-case AllocationError remains unchanged. Conditional ownership in both success and failure cannot be represented by pretending v2's Ok-only owner state suffices.

Before irreversible promotion, reserve destination graph/storage/tracing metadata and root-publication capacity; retain/root source across every safepoint. Failure must not publish partial graph state or lose ownership, including root-registration failure after payload preparation. Preserve prior user mutations. Charge successful reservations cumulatively to every active guard; cleanup never refunds them. Ref writes are region-invariant through all nested field and package/generic substitutions.

### Recoverable guard entry

`match memory.try_no_gc(budget: N):` with exhaustive Result arms establishes a scoped affine guard only in Ok. Budget is nonnegative and evaluated once; invalid dynamic values return ShapeLimit. Guard binding cannot escape or be manually ended early. Establishment failure returns allocation-free AllocationError before entering the arm. Body failures retain normal semantics; allocation recovery inside the body remains explicit. Preserve existing no_gc syntax/API behavior.

The same single Session, all regions and all active outer guards participate. Explicit or unknown/transitive collection rejects under a guard. Allocation-triggered collection is suppressed by the actual Session. Nested budgets charge every successful allocation, metadata/root/guard reservation and transfer before commitment. Guard/region/root cleanup never collects, and Manual region exit never triggers collection.

### Recoverable Session bootstrap

Entry main may opt into bootstrap by making its entire executable body an exhaustive direct match on `memory.try_session()`. This is an entry policy, not an ordinary eager call. It executes before any implicit Session construction. No statements before or after the match; main returns i64, Bool or Unit. For this entry policy, requires/ensures annotations on main reject explicitly, so no allocating contract can execute before entry or after failed-entry fallback. Contracts on ordinary functions executed inside Ok remain supported.

```cg
fn main() -> i64 effects(ManagedAlloc):
    match memory.try_session():
        Result.Ok(session):
            work()
        Result.Err(error):
            match error:
                memory.SessionEntryError.HostAllocation(site):
                    -10
                memory.SessionEntryError.IdentifierExhausted:
                    -11
```

Ok establishes exactly one Session; ordinary source calls execute within it, and all owned/root/region cleanup finishes before leaving the arm. Session capability cannot escape, be copied, stored, manually dropped or used to create another Session. Legacy implicit-entry programs retain behavior and diagnostics.

The error arm and all transitive callees admit only copied scalar operations and copied entry-error matching. No strings, show, arguments, owners, roots, regions, managed allocations, nested sessions or unknown resource summaries. Authenticate this restriction; ordinary semantic effects() alone is insufficient. SessionEntryError is separate from AllocationError: HostAllocation(site) and IdentifierExhausted. The copied versioned source reserve-site enum initially contains SessionRegions, matching the current runtime entry reserve; future sites require explicit additive versioning rather than unchecked strings.

Result dispatch/fallback need no CGx2 heap allocation or hidden replacement Session. All runtime setup required to enter Ok must fail before entry commits. Host compiler/evaluator bookkeeping is not claimed recoverable from arbitrary process OOM. Pre-Session evidence reports metrics as unavailable, never synthetic zeros. Preserve source spans and exact primary error identity.

## Complete connected ordinary ownership implementation

Generalize the current scalar-field-only path to concrete generic records, variants, List/Array/String and nested ownership-bearing Results; owned parameters/results, shared/exclusive loans, static field/index projections, partial moves and deterministic remainder destruction; branches/joins/matches/loops/recursive calls/contracts and checked package functions. Retain runtime call/step and analyzer bounds. Use conservative overlap for dynamic indices; no unchecked dynamic-index move or alias-to-owner conversion. New admission must not bypass checking of unused functions, recursive SCCs, private package bodies or imported transitive dependencies.

Return loans must derive from eligible surviving borrowed inputs, never local/consumed owners. Preserve loan provenance through projections, reborrows and generic calls. Recursive summary checking needs a sound bounded fixed point, not assumed trusted recursive summaries. Regions, aggregate owner state and both Result arms require a versioned analyzer with definite/possible liveness at joins. Ref-bearing ownership topology limitations need precise rejection/evidence and stay explicitly linked to the original incomplete acceptance row; do not silently narrow the source checkpoint.

One actual Session underlies mixed managed/owned/region/guard code. Authoritative graph/owner storage uses accounted runtime memory, not unaccounted host aggregate side tables. Preserve default heap roots through temporaries, arguments, returned values and cleanup; do not keep every value rooted until program exit. Existing 65,536 collection elements, 1 MiB UTF-8 strings and 65,537 total value-node envelope stay compatible. Checked construction/transfer bounds must be explicit and tested before large expansion.

## Additive interfaces and authentication

Preserve old exhaustive enums, struct literals, source APIs and v1/v2 defaults. Add `cgx2_ownership::v3::{Function,FunctionSummary,analyze_function}` with region quantification, aggregate places/loans, conditional success/error owners and authenticated callee summaries. Add `cgx2_sema::ordinary_memory::{MemoryProgramV1,MemorySignatureV1,EntryPolicyV1,MemoryEffectsV2}` and `cgx2_runtime::ordinary_memory::{OwnedGraphV1,PromotionFailureV1,MemoryEvidenceV1}`. Detail these owned contracts before implementation; additions must not reinterpret old values or trust caller-set acceptance flags.

MemoryEffectsV2 retains v1 may_allocate_managed, may_auto_collect_at_allocation, may_explicitly_collect and may_allocate_root_bookkeeping, and adds may_enter_session, may_create_region, may_enter_guard, may_allocate_owned and may_transfer_graph. All are inferred/authenticated transitively; unknown is never false. Bootstrap fallback requires all resource flags false, plus copied scalar/type restrictions. Preserve separate behavioral effects/capabilities and old effects() immutable-value behavior. New allocation/transfer/region/entry operations require the existing ManagedAlloc effect; explicit collection also requires existing Collect/MemoryCollect enforcement; mutation/read require existing effects. Include richer provenance/effect metadata in API/memory outputs with explicit version compatibility.

Reuse repaired Source, AstOnly and SourceBundle construction/authentication. Bind original UTF-8 inputs, bodies/layouts, generic substitutions, region and loan identities, root maps, conditional owner states, memory/behavioral effects, and transitive callee summaries; rebuild/check before execution/emission. AST-only output never advertises original-source authentication. Verify package freshness/artifact publication; hostile/stale/mismatched caller models must reject before output replacement.

Interpreter, native debug/release and standalone Rust share authenticated semantics and actual storage/cleanup. Preserve portable Python foundation value behavior; explicit unsupported memory/ownership semantics reject CGE006 before artifact replacement. Notebook/verifier/advisory consumers conservatively reject unimplemented new semantics before legacy walkers; no false proof/clean report. S3 adapters remain gated.

## Required acceptance and commands

Use private `$PWD/target/release-ordinary-memory` and `$PWD/target/tmp-ordinary-memory`; create them first. Set RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, LIBRARY_PATH=/tmp/cgx2-wave22-lib, PYTHONDONTWRITEBYTECODE=1. No shared target directory, package download or release timing on this host.

Run offline locked all-feature workspace tests, all-target/all-feature Clippy with -D warnings, cargo fmt --all --check, git diff --check. Finish all Cargo mutation, then final `cargo build --workspace --all-features --offline --locked`, freeze all seven compiler/consumer binaries and probe libraries using managed-core-repair-evidence/freeze.py. Verify their hashes before/after product checks; no concurrent Cargo writes during parity runs.

New bounded runner:
`python3 crates/cgx2-syntax/ordinary-memory-evidence/run.py --binary-dir FROZEN --output-dir NEW_ATTEMPT`

Replay every command in this plan's regressions.json with SOURCE_ROOT/BINARY_DIR/OUTPUT_DIR placeholders mapped to the worker and its private frozen artifacts. It preserves the unchanged historical shadow-builtin failure separately from reconciled593/restored17; no false pass or skipped negative. Run new evidence-harness selftests as well. Preserve every failing attempt and actual command/exit/signal/output count/hash; signals/timeouts/unrelated diagnostics do not satisfy expected errors.

Minimum new source/package acceptance:
- Generic aggregate ownership, projected loans/reborrows/partial moves, branches/loops/matching/recursion and failure continuations; invalid moves/overlap/escape and forged imported summaries reject.
- Named auto/manual regions and isolation, explicit copy with aliases and cycles, invariant Ref writes, destination closure, distinct dynamic brands and cleanup at early returns/errors. Default operations remain compatible.
- Zero/exact/one-under true allocation/root/guard/transfer budgets across nested guards and multiple regions; cumulative charges/no refunds, zero protected collections, no Manual exit/failure collection.
- Promotion success consumes once; every failure returns readable/retryable same owner and publishes no partial graph. Copy preserves source; forbidden topology rejection has exact diagnostics.
- Synthetic reserve failures at Session, region, guard, graph, root-publication, copy and promotion establishment, separately labeled from real budget limits. Source-reachable Session fallback and IdentifierExhausted coverage (direct runtime control where practical, never pretend global OOM recovery).
- Normal/return/failed-argument/contract/arithmetic/allocation/cancellation/unwind cleanup; original error preserved, root/owner census back to baseline and no double release. Unsupported external kill guarantees described precisely.
- Exact interpreter/native debug/native release/standalone values/errors/UTF-8 spans and deterministic memory observations. Standalone runs outside repository assumptions. Public API/source/package authentication and artifact-preserving Python/static rejection.
- Runtime state-machine/fault/conformance tests for changed paths. Retain source/runtime/compiler/toolchain/binary hashes and audit-ready reproduction. External leak-enabled checks and independent audit remain separate unrun/blocked evidence; disabled-leak ASan is not acceptance.

Complete the connected implementation and evidence, commit and stop for coordinator review. Report exact SHA, ownership, APIs/defaults, observed counts, required root-lock reconciliation and any missing row. A rejection cannot count as an implemented required capability. No full ordinary-memory claim until coordinator reviews every row. Typed foreign/package/tensor/owned map-zip execution, installed commands, S3/S4, real library/GPU/solver/isolation/leak/performance and paired human/model studies remain required; do not mark the project complete.
