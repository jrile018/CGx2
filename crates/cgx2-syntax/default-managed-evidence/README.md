# Default managed checkpoint evidence

This is the bounded R03/R04 deliverable based on
`c9d60229ecf3ed2bf76b29525fec0bdda41d0906`. It is ready for coordinator
review when the checks selected by `summarize.py` pass. It does not close the
full S2b memory checkpoint, S3/S4, or any R01–R15 release gate.

## Implementation

Ordinary records, variants, List, Array and String now store authoritative
payloads and traced edges in the real default `Session`. The default automatic
region still has a 1 MiB collection trigger and 64 MiB limit. Immutable updates
allocate new values and preserve earlier aliases. `managed.Ref<T>` is the
explicit mutable cell boundary; recursive graphs are admitted only through
Ref indirection. Graph counting uses visited handle identities; public snapshots
retain independent expansion, depth, element and UTF-8 byte bounds. Ref-bearing
public outputs and Eq/Show constraints reject before execution.

`ManagedProgramV1` in `cgx2-sema/src/default_managed` owns a private checked
source, typed module, concrete model, layouts and root maps. Authentication
reconstructs and compares these, binding behavioral effects, inferred
`MemoryEffectsV1`, bodies and callees to separate source/analysis SHA-256 values.
The interpreter and generated native/standalone Rust execute the same checked
model through the shared runtime engine. Native output embeds the complete
runtime and model; it needs no repository-relative runtime dependency. Models
above 2,048 expressions use the closed, versioned `CGMD\x01` wire codec rather
than a huge Rust syntax tree. The producer round-trips the entire model before
emission. This repaired the retained first 65,536-element native build failure
under the 4 GiB address-space bound.

`ScopedRoots` now supports checkpointed temporary release and guarded ownership
of its registration table. Parameters, locals, earlier arguments/children,
match payloads and returned handles survive allocating safepoints. Overwritten
bindings and discarded block tails release promptly. Explicit `finish` preserves
the primary result and records cleanup errors; no root/guard cleanup collects.
The runtime-only tests cover root transfer, normal guard cleanup and Rust unwind.

`managed.try_alloc(EXPR)` evaluates the entire expression inside recovery,
including root registration. Ok/Err use allocation-free intrinsic carriers.
Only the five frozen allocation errors are caught; arithmetic/contracts remain
primary errors. Real zero/exact/one-under, nested and cumulative nonrefund guard
budgets, real RegionLimit recovery, and labeled synthetic reserve failures are
covered. Session/guard entry failures remain uncaught, source-located errors.

The mixed envelope `HybridProgramV1` authenticates the original source, old v2
projection, concrete managed helpers and copied-scalar bridge. i64/Bool/Unit
helper arguments/results borrow the caller's actual Session and active guard.
Scalar-only owned initializers retain the old inline semantics. Ordinary heap
helpers need no explicit managed marker. Managed handles/owners cannot cross
this boundary. Direct affine captures, mixed packages and ReadArgs across this
bridge reject explicitly pending later ownership/argument gates; no empty
argument list or second unguarded Session stands in for those operations.

## Compatibility record

- Existing syntax/value/ownership v1/v2 enums and schemas, including old API
  schemas 1–7 and schema-1 memory reports, remain readable. The new model and
  `cgx2 memory-effects FILE|PACKAGE` API are additive. Its schema-1 summaries
  explicitly cover concrete specializations; an absent summary is unknown,
  never evidence of allocation-free execution. The mixed owned entry is not
  represented by a synthetic managed stub in this command; mixed indexing is
  rejected pending a combined index gate.
- Ordinary successful executions now return actual Session memory reports.
  Behavioral `effects()` remains valid for immutable construction but says
  nothing about allocation. Explicit cells/reads/writes/collection retain their
  separate behavioral requirements. Unknown or explicit transitive collection
  rejects under no_gc; automatic collection is suppressed by the actual guard.
- Source checks retain target-literal portability deferral. Unsupported escapes
  still fail interpreter execution with CGR009, Rust emission/build with
  CGRS003, and Python emission with CGE003 before artifact replacement. Foreign
  declarations in legacy packages retain their previous admission rules;
  combining new managed observables and foreign execution remains rejected.
  Legacy malformed scalar argument diagnostics and CGR011 recursion errors are
  preserved. Valid arg() String values use managed storage.
- Python retains portable immutable values, including the 1 MiB Unicode case.
  Its UTF-8 work counter is now a separate cumulative 256 MiB bound, while its
  node/step work bound remains 1,000,000. Explicit cells, collection, guards and
  owned observables reject before replacement. User-defined imported
  `managed.ref` remains an ordinary function on every supported target.
- Notebook, verifier and advisory effect consumers conservatively gate the new
  observables instead of producing old proof/clean reports. Their full S3
  integration remains open.
- Emitter representation assertions now inspect authenticated model/layout
  identities instead of obsolete generated host aggregate structs. The Python
  record test harness uses unique temporary names so concurrent cases cannot
  overwrite one another. Assertions were retained.

## Reproduction and evidence

Use the pinned environment from the plan, with offline/locked dependencies:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-default-managed"
export TMPDIR="$PWD/target/tmp-default-managed"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
python3 crates/cgx2-syntax/default-managed-evidence/verify.py --output-dir /tmp/managed-review
```

`run.py --only NAME` runs one product fixture. `verify.py --only NAME` runs one
required check (or a comma-separated set). `summary.json` selects final successful attempts, preserves the
known superseded historical negative and blocked leak runs, and links retained
failed attempts. Commands, exits, signals, limits, stream hashes, binary/runtime
hashes, source fixtures and memory observations are retained. Generated ELF/rlib
artifacts are replaced after execution by a size/hash inventory; large generated
source/log files are gzip-compressed without changing their bytes. No historical
runner, fixture or evidence file was edited. The coordinator's unchanged
`reconcile.py` is used for the superseded shadow-builtin negative.

Product commands have a 4 GiB per-process address-space bound, 120 CPU seconds,
180 wall seconds and 2 MiB retained per stream. Integration checks have 900 CPU
and wall seconds and 8 MiB combined retained output. These are per-process
bounds and provide no aggregate cgroup isolation. ASan needs sparse multi-terabyte
shadow address space: sanitizer runs use its 4,096 MiB per-process RSS monitor,
900 CPU/wall seconds and a 64 MiB file-size cap instead of RLIMIT_AS. Its RSS
polling is not a strict peak bound. The earlier cap-induced failures are retained.
Leak-enabled results are blocked when the deliberate leak negative control
cannot run; no-leak ASan passes are not leak acceptance.

The source evidence field `allocations` counts logical source graph values,
not host allocator calls. `roots_registered`/`roots_released` count compiler
registrations, excluding graph-builder internal temporary registrations. Actual
committed/live/peak bytes, collection counts/bytes and allocation failures come
from Session reports; final/baseline root census is also read from Session.
Synthetic fault rows are identified separately. Product diagnostic parity
compares stable error codes/messages and deterministic memory observations;
unit tests separately verify original UTF-8 expression spans. Formatting and
source path prefixes differ between CLI and standalone diagnostics.

## Bounded acceptance matrix

| Required row | Evidence and limit |
| --- | --- |
| Five shapes, concrete generics, package calls, immutable updates | Product `shapes`, `package`, `wide`, `elements`, `unicode`; semantic layout and persistent-value tests |
| 65,536 elements; 65,537 logical nodes; 1 MiB UTF-8 | Product generated source retained gzip; exact node boundary succeeds and a further append recovers ShapeLimit |
| Live locals/earlier arguments/returned roots; dead values reclaim | Product `pressure`; 20,000-allocation loop; normal/early return tests; actual collection bytes and final census |
| Ref aliases and real cyclic graphs | Product `alias`, `cycle`; 100-link cyclic graph test exceeds snapshot depth while remaining internal; unreachable graph collects |
| Real guard/recovery accounting | Zero/exact/one-under allocation (226/225) and root bookkeeping (64/63); nested outer 258/257, cumulative nonrefund; `guard-package`, `hybrid`, real `region-limit` |
| Primary errors and cleanup | Product arithmetic, contracts, failed arguments and negative arg; exact code/message and memory parity; UTF-8 span test; final roots equal baseline; runtime unwind/drop tests |
| Fault injection | Labeled synthetic reserve indices 2–15, runtime/conformance historical fault corpus; failures outside protected entry remain uncaught rather than claimed recovered |
| Targets and memory/effect compatibility | Interpreter/native debug/native release/independent standalone; actual report parity; `memory-effects` concrete summary parity; portable Python and static artifact preservation |
| Consumer gates and user namespaces | Workspace notebook/verifier/effect tests; negative explicit Ref/collect cases; dishonest package; imported user `managed.ref` on Rust/interpreter/Python |
| Required regressions and sanitizer | Selected final attempts in `summary.json`; superseded historical negative retained separately; ASan without leak detection passes, leak-enabled acceptance remains host-blocked |

## Retained gates

Named regions, cross-region transfer, failed-owner transfer/entry recovery,
broader generic/package ownership, recursive/partial moves and reborrows,
foreign/tensor/map-zip/worker execution, installed commands, external audit,
leak-enabled acceptance and performance measurements remain open. Existing
cooperative library cancellation/unwind cleanup is covered by the lifetime
corpus; no new source cancellation protocol is claimed. Ref-bearing output,
non-copied mixed boundaries and mixed ReadArgs are explicit rejection boundaries.
No full ordinary-memory, release-complete, leak audit or performance claim is made.
