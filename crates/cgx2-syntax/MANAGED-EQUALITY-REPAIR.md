# Managed Result logical equality repair

Reviewed repair base: `3c1cfe2aeba2f7135e4069709bf633c082d2aab0`.
This is one bounded repair for coordinator review. The original managed delivery
remains staged and unaccepted. R03/R04, full ordinary memory, and all broader
release gates remain open.

## Behavior and compatibility

`crates/cgx2-runtime/src/graph.rs`, `Machine::equal`, now compares logical type,
case, arity and recursively compared payloads across intrinsic recovery carriers
and ordinary heap variants. Previously, differing `Value.depth` rejected equal
values before reading their logical cases. Equal `managed.try_alloc(1)` and
`Result<i64, AllocationError>.Ok(1)` now compare equal, in both operand orders;
`!=` returns the inverse. The same rule applies to Err, unequal carrier depths,
and values nested inside records, variants, List and Array.

Type identity remains the first check. Heap identity and visited-pair shortcuts
apply only when both values have carrier depth zero: a wrapped handle identifies
its payload, not the enclosing variant case. Existing `parts` decoding reads the
actual stored payloads and carrier layers; equality constructs no managed
variants, roots, or recovery carriers. Scratch decoding uses existing host
vectors. The existing 65,537-node work limit, depth-64 bound, step accounting,
heap-pair tracking, checked failure propagation, and cleanup paths remain active.
No public model/type/API shape, source admission, managed allocation, root,
guard charging, collector, persistent update or intrinsic constructor changes
are part of this repair.

## Verification

All commands use pinned Rust 1.98.1, `CARGO_BUILD_JOBS=2`, offline/locked existing
dependencies, `LIBRARY_PATH=/tmp/cgx2-wave22-lib`, and private directories
`target/release-managed-equality-repair` and `target/tmp-managed-equality-repair`.
Root Cargo.toml/Cargo.lock match the base. No dependencies were added.

| Check | Result |
| --- | --- |
| Runtime, sema and memory-conformance all-feature tests | 307 passed, 0 failed, 0 ignored |
| New runtime tests | 4 passed; both operators/orders, equal/unequal Ok and Err payloads, real budget and ShapeLimit failures, nominal types, nested carrier depths, depth-bound failure and root cleanup |
| New typed source tests | 2 passed; four source fixtures plus exact CGT039 rejection for Ref-bearing and differently typed comparisons |
| New bounded parity runner | 87 checks passed; four fixtures on interpreter, native debug, native release and standalone Rust |
| Unchanged graph / guard / report runners | 99 / 518 / 265 checks passed |
| Unchanged default-managed product runner | 328 checks passed |
| Affected all-target/all-feature Clippy, warnings denied | Passed: runtime, sema, memory-conformance, CLI, Rust emitter and cgx2c |
| Workspace format and diff whitespace checks | Passed |

The four typed fixtures cover scalar Ok, zero-budget Err, nested records/variants/
collections, nested intrinsic Result layers and String payloads, persistent
append, reversed operands, both operators, and non-equal payload/case controls.
Runtime model tests additionally compare copied BudgetExceeded versus ShapeLimit
errors and different nominal type IDs. Typed source tests require `CGT039` for
direct/nested Ref equality and different nominal/Result types; parse errors do
not satisfy these negatives.

The new parity runner uses the shared bounded process supervisor: each child has
180 seconds wall time, 120 seconds CPU, 4 GiB virtual address space and 2 MiB
retained output per stream. It checks process reason and exit; signals, timeout,
spawn failure and overflow never count as success. These are per-process limits,
not aggregate cgroup isolation. Each native/standalone result must equal 42 and
match the interpreter's complete managed evidence/memory observation, excluding
only session IDs and collection pause timings. Roots return to zero, all root
registrations are released, cleanup failures are zero, and collection counts are
zero. Err fixtures verify the exact expected real recovery counts (2 and 1).

Standalone files embed the runtime and compile directly with pinned rustc in a
private temporary directory, without Cargo dependencies or repository includes.
The runner rejects include!/include_str!/#\[path\]/extern-crate dependencies and
retains emitted source and artifact hashes. It uses the existing bounded linker
thread setting; no installations or host settings are changed.

## Retained attempts and reproduction

`managed-equality-repair-evidence/summary.json` indexes final gates, exact command
records, source/log hashes, unchanged manifests, and earlier failed attempts.
`evidence/parity-final/product.json` retains every new target command, exit/signal,
stdout/stderr and observation. Historical product outputs are retained separately
from command records; existing evidence was not edited.

Valid new tests run against unchanged graph.rs reproduce the defect: three
runtime tests fail and the typed Ok fixture returns 1 instead of 42. Earlier
fixture syntax/compile mistakes and a test-child SIGABRT are retained as failures.
The abort came from the initial recursively constructed depth test exhausting the
Rust test stack; replacing the setup with local bindings now tests CGR010
`equality work bound exceeded` with root cleanup. It is not counted as a pass.
An initial recorder filename collision also failed the outer orchestration after
the three historical child products passed; separate-path reruns retain complete
command records. Exact details are in the summary's attempt notes.

From the repository root, use fresh output directories/attempt labels:

```sh
python3 crates/cgx2-syntax/managed-equality-repair-evidence/check.py tests-rerun -- cargo test -p cgx2-runtime -p cgx2-sema -p cgx2-memory-conformance --all-features --offline --locked --no-fail-fast
python3 crates/cgx2-syntax/managed-equality-repair-evidence/check.py parity-rerun -- python3 crates/cgx2-syntax/managed-equality-repair-evidence/run.py --binary-dir target/release-managed-equality-repair/debug --output-dir target/tmp-managed-equality-repair/parity-rerun
python3 crates/cgx2-syntax/managed-equality-repair-evidence/summarize.py
```

The runner and command recorder preserve failed attempts rather than replacing
existing output directories. No new broad validation framework or performance
runs were added. Full workspace tests and external leak-enabled/audit/performance
acceptance were not rerun for this bounded repair. The separate provenance,
nominal-compatibility and evidence-parity lanes remain coordinator review inputs;
this repair does not accept those interfaces or close the managed checkpoint.
