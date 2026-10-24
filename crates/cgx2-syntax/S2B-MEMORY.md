# Ordinary-function memory checkpoint

This work starts at reviewed foundation `c110fd5f9ae140aa985188ca1ab526f38e15a7ca`.
The checkpoint is **incomplete until every source/target acceptance row below
has executable evidence**. Foundation acceptance is not memory acceptance.

## Runtime representation contract

The existing `Session`, stable session/region/generation handles, collection
policies and all existing enums keep their meaning. Default managed storage
remains auto, trigger 1 MiB, limit 64 MiB. Owned storage remains unique;
managed graphs can share children and contain cycles. The v1 ownership
analyzer's unique region-bound values cannot certify those aliases.

Payload initialization and updates must use checked byte ranges and validated
object handles. A new, separate error type reports payload bounds without
adding variants to the existing public exhaustive `MemoryError`. Writes neither
allocate nor collect; a rejected write leaves all bytes unchanged. Typed
lowering is responsible for authenticating the layout that interprets bytes.

Graph constructors must validate and root every evaluated child before the
first allocating safepoint, including children beyond the first chunk. No raw
pointer or host object table supplies CGx2 storage. A bounded chain of ordinary
runtime objects accommodates 65,536 children while preserving the existing
256-reference-slot and 1-MiB-payload bounds per object. Chunk links are traced
references; the public result is rooted only after the caller registers it.
Temporary root cleanup does not collect, close a region or replace a primary
allocation/initialization failure. Partial graph construction can leave
unreachable committed objects until a permitted collection: it does not
promise allocation rollback or refund cumulative no_gc budgets.

Every allocation, root-table reserve and guard reserve uses existing fallible
entry points and session-wide nested budgets. Named manual regions collect
only on explicit collect. `no_gc` rejects explicit collection in every region;
guard and root cleanup do not collect manual regions.

## Source interface and acceptance obligations

Ordinary functions require compiler-resolved owner/loan/layout identities,
original UTF-8 spans, authenticated complete callee summaries and full CFG
lowering into `cgx2_ownership::v1::analyze_function`. Copied scalar parameters
must not be represented as owned inputs; Result failure paths must retain
the input owner. No source mode is admitted merely because a Rust runtime API
exists. The implementation inventory below distinguishes executable source
from supporting runtime APIs.

Required source coverage remains ordinary parameters/results/locals,
projections and partial moves, branches/loops/matches/early returns, recursive
SCCs and concrete generics, checked packages, default managed
records/variants/List/Array/String including aliases/cycles, region transfers,
recoverable allocation/conversion errors, deterministic cleanup and session-wide
no_gc. Python must preserve its finite portable profile and reject unsupported
memory observables before artifact replacement. Unsupported consumers must
reject before old walkers. No root Cargo files or dependency additions are
authorized in this checkpoint.

## Implementation and verification inventory

Updated with exact commands, source/target results, remaining gaps and commit
identity after implementation. Until then this is a contract, not acceptance
evidence.

### Implemented source grammar and target admission

The additive explicit memory-script operations are:

```cg
let graph = memory.alloc_graph(region, payload_bytes, reference_slots)
let reference = memory.graph_reference(graph, literal_slot)
memory.set_graph_reference(graph, literal_slot, another_object_or_graph)
memory.clear_graph_reference(graph, literal_slot)
memory.write_graph_byte(graph, literal_offset, literal_byte)
memory.expect_graph_byte(graph, literal_offset, literal_byte)
memory.write_byte(object, literal_offset, literal_byte)
memory.expect_byte(object, literal_offset, literal_byte)
```

Offsets/slots/bytes are nonnegative integer literals; bytes must be 0..255.
`alloc_graph` allows 1 MiB/65,536 logical bytes/slots; `alloc_object` keeps its
1 MiB/256-slot limits. Raw object and graph accessors cannot be mixed. Static
checking validates bounds, child availability and same-region provenance.
An expectation mismatch is the original source-spanned `CGR015`; allocation and
budget failures retain `CGR013`. These are uncaught script errors, **not a new
source Result conversion API**. Evaluation order remains the existing memory
script order. Graph operations inside memory loops and conditional declaration
profiles remain conservatively rejected by the existing script checker.

Any module containing `memory.alloc_graph` selects fallible `Session::try_new`
and `try_no_gc` for its whole invocation, including all scalar memory callees.
This charges guard-table bookkeeping to enclosing guards and suppresses
collection session-wide. Existing nongraph scripts retain their compatibility
entry points and accounting; this does not silently change earlier exact-budget
fixtures. Every graph constructor roots all evaluated children and intermediate
chunks using scoped roots. Source bindings use existing checked memory-script
root registrations and cleanup. Source reports enumerate logical graph edges,
including slots beyond 256; they do not mistake internal chunk links for source
edges.

| Target/path | This implementation |
| --- | --- |
| Source check, interpreter | Explicit graph/payload operations in scripts and existing scalar-parameter Unit-result memory functions |
| Native debug/release | Same operations, errors, reports and session policies via authenticated embedded `cgx2-runtime` |
| Emitted standalone Rust | Requires the authenticated runtime rlib; the runner stages and hashes every runtime module, then compiles/runs the emitted source independently |
| Python | Explicit graph/memory operations reject before artifact replacement; existing portable copied values remain admitted |
| Ordinary generic/data value functions and packages | Their foundation copied-value semantics are unchanged; no new ordinary memory admission |

### Incomplete required acceptance (not release-ready)

The assigned ordinary-function checkpoint is **not complete**. In particular:

1. There is no source lowering of ordinary owner/loan parameters/results or
   partial projections into `v1::analyze_function`; the explicit script path
   still uses the legacy operation checker. Authenticated ordinary ownership
   summaries, recursive CFGs, loan returns and ownership-bearing Result branch
   provenance are not implemented.
2. Ordinary records/variants/List/Array/String still use the accepted foundation
   copied host-value representation. The graph storage above is real traced
   runtime storage, but it is selected by explicit memory intrinsics. It does
   not implement default managed storage/roots for those ordinary values.
3. Ordinary recoverable region allocation/conversion Result continuations,
   source owner preservation across failed transfers, ordinary cleanup and
   cooperative cancellation, package memory calls and generic no_gc effects
   remain unimplemented. Existing library and script tests are not evidence
   for those missing paths.

The concrete integration gap is the current split: `cgx2-sema/src/memory.rs`
admits only scalar-parameter Unit-result memory functions, whereas
`cgx2-sema/src/foundation.rs`, `cgx2-cli/src/eval.rs` and the finite emitters use
copied values with no session/owner/loan representation. Completing the assigned
checkpoint requires actual checked ordinary memory types and resolved layouts,
source/body-authenticated CFG summaries (with original per-module source spans),
a shared ordinary managed value/root representation using the runtime, and
interpreter/native/package lowering of ownership and recoverable error edges.
Rejecting those programs safely is not implementation of their required
capability. No external prerequisite makes those compiler gaps complete.

Foreign/tensor/package-environment/owned-map execution, command closure, S3,
external leak-enabled/independent audit/solver/isolation/performance and paired
human/model gates remain mandatory as in the assigned plan. No shared design,
ledger, root manifest/lock, foreign/execution lane or security policy is edited.
No dependency was added; root-lock reconciliation is **none**.

### Verification and reproduction

Enclosing Git commit is the delivery identity (`git log -1 --format=%H --
crates/cgx2-syntax/S2B-MEMORY.md`). [Machine-readable verification](s2b-memory-evidence/evidence/verification.json)
records exact scope, base, commands, log hashes, root file hashes and preliminary
failed invocations. [Product evidence](s2b-memory-evidence/evidence/graph-product.json)
retains every new source/target command and result. These are supporting
implementation results; `checkpoint_complete=false` and `release_complete=false`.

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-s2b-memory"
export TMPDIR="$PWD/target/tmp-s2b-memory" LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo build --workspace --offline --locked
cargo test --workspace --all-features --offline --locked
cargo test -p cgx2-runtime --no-default-features --offline --locked
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
cargo fmt --all --check
git diff --check
python3 crates/cgx2-syntax/s2b-memory-evidence/run.py \
  --binary-dir "$CARGO_TARGET_DIR/debug" --output "$PWD/target/s2b-memory-product.json"
python3 -m unittest discover -s bench/tests
python3 -m unittest discover -s tools/docs-check
```

Results: **1,157 passed, 0 failed, 6 ignored**, 254 workspace suites; runtime
without features **86 passed**; Clippy/format/diff/default workspace build pass.
The new product runner passes **99 checks**: five real source programs (three
successes, two uncaught runtime errors), six generated rejected programs, all
four executable targets and preserved Python rejection artifacts. Product
processes run in a private working directory and stage the authenticated runtime
independently; there are no repository-relative runtime source assumptions.
The ten runtime graph tests, five source graph tests and independent 28-trace
logical-slot model are separate from source acceptance counts. Test/fault
injection and address instrumentation are synthetic/library evidence where
labeled, not proof or ordinary ownership capability.

All seven foundation product regressions remain passing: foundation **159**,
semantic resolution **278**, source specialization **207**, public API **118**,
package binders **88**, output bounds **31**, emitters **104**. Exact invocations
are in [regression commands](s2b-memory-evidence/evidence/regression-commands.json).
The benchmark and docs harness unit suites pass **166** and **20** tests; these
are harness checks, not performance measurements or release studies.

```sh
python3 crates/cgx2-memory-conformance/scripts/accept_lifetimes_asan.py \
  --output "$PWD/target/s2b-memory-asan-leaks"
# Separately labeled address-only instrumentation, never leak acceptance:
python3 crates/cgx2-memory-conformance/scripts/accept_lifetimes_asan.py \
  --output "$PWD/target/s2b-memory-asan-no-leaks" --detect-leaks 0
```

Leak-enabled result is **blocked (exit 2)**: the deliberately leaking control
hits `LeakSanitizer has encountered a fatal error` and its ptrace restriction
hint. The leak corpus is unrun. An external host must allow the negative control
to be detected before the corpus is admitted; no installation or security
changes were made here. Separate `detect_leaks=0` ASan passes **23 tests** of
runtime units/lifetimes/graphs and independent conformance models; stdlib is not
instrumented. [Blocked evidence](s2b-memory-evidence/evidence/asan-leaks/result.json)
and [non-leak evidence](s2b-memory-evidence/evidence/asan-no-leaks/result.json)
are retained. Independent safety audit remains unrun.

The existing interpreter binding/evaluation tables retain compatibility
bookkeeping; this explicit script extension does not establish complete
ordinary-source allocation accounting. All missing ordinary-source admission
rows above remain mandatory work inside the assigned checkpoint.
