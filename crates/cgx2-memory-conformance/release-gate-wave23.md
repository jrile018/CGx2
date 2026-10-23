# Wave 23: automatic collection pressure and recoverable failures

Base: `9b8f6f5610e697429b364528fa32a6b716aa288a`. This is finite direct-runtime evidence, not a memory-safety release, proof, or leak-freedom claim. No production runtime defect was demonstrated, and no production runtime semantics changed.

## Implementation and scope

The shared oracle is `crates/cgx2-runtime/tests/support/automatic_pressure.rs`, instantiated by `automatic_pressure_wave23.rs` in runtime and memory-conformance. It uses public `Session`, `Handle::generation`, root-census, stats, and memory-report APIs. Expected state comes from logical node IDs, a separately maintained generation/occupancy table, graph traversal, allocation charges, budget balances, and specified safepoints. Runtime results are compared with predictions; they do not decide expected success, collection, reclamation, roots, or generations. Real returned handles are stored separately to address the runtime objects.

Each seed selects which region is automatic and an automatic threshold of `(2 + seed % 3) * C`; the other region is manual. `C = SLOT_METADATA_BYTES + 1 + REFERENCE_SLOT_METADATA_BYTES` and each region's limit is `12 * C`. Payload lengths vary from 1 to 7. A short pressure prologue establishes roots, duplicate registrations, a cycle and checked reference, followed by a SplitMix64-generated sequence. The tail varies allocation, root/unroot, checked references, legacy links, copied/mutated owned payloads and promotion, explicit collection, nested guards, and close order. Half the selections prefer currently live logical nodes; other selections retain stale/error-path coverage. Close attempts enter the last quarter of the generated tail. Seed-selected cleanup drains remaining roots and closes both regions.

After each primitive, including primitives inside nested guards, the oracle checks both regions' committed/live/peak bytes, allocation failures, collection/reclamation counts, report rows/totals and pause sample counts, global/per-region root census, payloads, checked references, generations, and all historical stale handles. Legacy links are checked through retention/reclamation and charges because there is no public edge enumeration API. Copy payloads are predicted from model bytes and mutated independently; failed promotions must return the entire unchanged owned payload.

The model assumes the current host's exact reservation charges for these small `try_reserve_exact` payload/reference/edge vectors; it imports public metadata charge constants. It does not infer capacity charges from allocation results. Slot placement follows the runtime's documented implementation convention of first reusable vacancy. This is an implementation-conformance oracle, not an allocator-independent language proof.

The only existing test adjustment gates `AllocationError`'s import in runtime `tests/transfer_replay.rs` with `fault-injection`, allowing normal-feature Clippy to pass. No source syntax, compiler, CLI, concurrency, root Cargo file, or shared design document changed.

## Failure ordering and targeted cases

| Reserve / failure | Position and checked result |
| --- | --- |
| `ObjectPayload`, `ObjectReferences` | Before automatic collection: threshold pressure does not collect, mutate the graph, or commit the failed allocation. |
| `ManagedBytes` | After the automatic safepoint: failure can increment collection count and reclaim an entire unrooted cyclic graph. Rooted graphs and the unrelated manual region survive. |
| `SlotTable` | After the safepoint: fresh table plus `trigger_bytes=0` guarantees actual growth. Byte and object failures commit no object/generation; retry starts at generation 0. A collection that frees an existing slot would instead reuse it and need no table growth. |
| `Edges`, `ManagedToOwned` | No collector safepoint: injected failure preserves existing checked references, roots, payloads, and the other region. |
| `OwnedBytes`, `RegionTable` | Session-wide reserves do not charge a region failure or collect. The region-table case uses a fresh session's full one-element default-region table, avoiding assumptions about later spare capacity. |
| Nested budget exhaustion | Both `(outer=C, inner=3*C)` and `(outer=3*C, inner=C)`, with both region orders: the second allocation fails before any reserve, and explicit collection is forbidden in both regions. Exiting either guard does not collect; retry outside the guards collects only the automatic region. |
| Rooted region limit | Filling and rooting all `12*C` bytes permits an automatic cycle followed by `RegionLimit`, without reclaiming roots or committing the failed allocation. |

`first_failure` records the first predicted error with `get_or_insert_with`; targeted sequences check it survives subsequent different errors and successful retries. `corrupted_oracle_is_rejected` deliberately increments expected committed bytes and requires the common checker to reject the divergence.

## Corpus and achieved coverage

The committed corpus has 20 explicit decimal seeds in `crates/cgx2-runtime/corpus/wave23-pressure-seeds.txt`, including 0, adjacent values, large values and `u64::MAX`. Default runs generate 128 tail operations per seed. Expanded runs use decimal start seed 0, count 2,048, and 256 tail operations per seed, with wrapping u64 seed arithmetic. Configuration bounds are count <= 10,000 and steps <= 4,096; expanded work is opt-in. No elapsed-time gate substitutes for completed transitions.

| Run | Auto cycles | Explicit cycles | Reclaimed objects | Rooted survivors across cycles | Reused slots | Reserve failures before collection / after collection | Budget failures |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Fixed, normal | 167 | 230 | 1,178 | 291 | 981 | 0 / 0 | 178 |
| Fixed, fault feature | 160 | 227 | 1,087 | 287 | 880 | 83 / 11 | 184 |
| Expanded, normal | 31,959 | 40,980 | 233,346 | 77,149 | 213,249 | 0 / 0 | 36,183 |
| Expanded, fault feature | 28,193 | 40,837 | 213,618 | 71,199 | 193,130 | 18,286 / 1,959 | 36,554 |

These collection counts are predictions validated against actual runtime counters after every primitive. The reserve counters describe allocation failures in the generated corpus; the separate targeted eight-site fixtures are additional evidence. Manual counters are checked at every step and can change only on a successfully modeled explicit collect. `audit-wave23.json` records complete top-level transition-pair counts and validated success/error outcomes, including nested primitives. Copy outcomes include both the source-copy and destination-promotion phases. The printed trace and coverage distinguish attempted transition pairs from checked outcomes.

A committed executable-operation trace for seed 7 / 24 generated steps / fault feature is `crates/cgx2-runtime/corpus/wave23-pressure-trace.txt`. Replay and trace export:

```sh
CGX2_W23_SEED=7 CGX2_W23_COUNT=1 CGX2_W23_STEPS=24 CGX2_W23_TRACE=1 cargo +1.98.1 test -p cgx2-runtime --features fault-injection --test automatic_pressure_wave23 --offline pressure_corpus -- --nocapture
CGX2_W23_SEED=0 CGX2_W23_COUNT=2048 CGX2_W23_STEPS=256 cargo +1.98.1 test -p cgx2-runtime --test automatic_pressure_wave23 --offline pressure_corpus -- --nocapture
CGX2_W23_SEED=0 CGX2_W23_COUNT=2048 CGX2_W23_STEPS=256 cargo +1.98.1 test -p cgx2-runtime --features fault-injection --test automatic_pressure_wave23 --offline pressure_corpus -- --nocapture
CGX2_W23_SEED=0 CGX2_W23_COUNT=2048 CGX2_W23_STEPS=256 LIBRARY_PATH=/tmp/cgx2-wave22-lib cargo +1.98.1 test -p cgx2-memory-conformance --test automatic_pressure_wave23 --offline pressure_corpus -- --nocapture
```

All four commands passed. The last command's counters match expanded fault-runtime counters.

For a failure, the harness stops at the first mismatching primitive and prints its decimal seed, generated step count, first modeled failure, executable `vec![Op::...]` prefix and a replay command retaining the feature selection. Keep `CGX2_W23_STEPS` fixed, because close scheduling depends on it. Set `CGX2_W23_COUNT=1` and binary-search `CGX2_W23_STOP` from 0 to the printed prefix length: prefix `p-1` passes and `p` fails. STOP applies to cleanup too, and intentionally omits completion coverage assertions. This gives a deterministic shortest failing prefix, not a globally minimal graph. To shrink further, paste the printed vector into a local test inside the shared module, use `Runner::new(seed)` and `step(op)`, remove contiguous chunks while preserving allocated node-ID dependencies, and retain each deletion only if the same mismatch remains. Restore the test file before committing unrelated work. No automated global delta-minimization claim is made.

## Verification

Host: `rustc 1.98.1 (48a229cea 2026-09-01)`; sanitizer toolchain: `rustc 1.101.0-nightly (5c543b0b8 2026-09-29)`. Commands ran offline. Conformance uses the existing `/tmp/cgx2-wave22-lib/libpython3.14.so` linker symlink to the host's installed versioned library. This workaround does not change repository files or establish a Python-free process.

| Command | Result |
| --- | --- |
| `CARGO_INCREMENTAL=0 cargo +1.98.1 test -p cgx2-runtime --offline` | Twice: 69 passed, zero failed/ignored per run, 14 test/doc-test binaries. |
| `LIBRARY_PATH=/tmp/cgx2-wave22-lib CARGO_INCREMENTAL=0 cargo +1.98.1 test -p cgx2-memory-conformance --offline` | Twice: 60 passed, zero failed/ignored per run, 22 binaries. |
| `CARGO_INCREMENTAL=0 cargo +1.98.1 test -p cgx2-runtime --features fault-injection --offline` | 74 passed, zero failed/ignored, 14 binaries. |
| `cargo +1.98.1 test -p cgx2-runtime --test automatic_pressure_wave23 --offline -- --nocapture` | 4 passed; normal fixed corpus above. |
| Same model command with `--features fault-injection` | 6 passed; fault fixed corpus above. |
| `LIBRARY_PATH=/tmp/cgx2-wave22-lib cargo +1.98.1 test -p cgx2-memory-conformance --no-default-features --test automatic_pressure_wave23 --offline -- --nocapture` | 4 passed; no model injections. The conformance manifest still unconditionally enables the runtime dependency's fault-injection feature, so this is not evidence of a feature-free runtime binary. The separate normal runtime run supplies that evidence. |
| Same conformance model command with `--features fault-injection` instead of `--no-default-features` | 6 passed. |
| `cargo +1.98.1 fmt --all --check` | Passed. |
| `cargo +1.98.1 clippy -p cgx2-runtime --all-targets --no-default-features --offline -- -D warnings` | Passed after gating the pre-existing fault-only test import. |
| `LIBRARY_PATH=/tmp/cgx2-wave22-lib cargo +1.98.1 clippy -p cgx2-runtime -p cgx2-memory-conformance --all-targets --features cgx2-runtime/fault-injection --offline -- -D warnings` | Passed. |

## Dynamic instrumentation and remaining blockers

```sh
RUSTFLAGS=-Zsanitizer=address ASAN_OPTIONS=detect_leaks=0 CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/tmp/cgx2-w23-asan CGX2_W23_SEED=0 CGX2_W23_COUNT=2048 CGX2_W23_STEPS=256 cargo +nightly test -p cgx2-runtime --features fault-injection --test automatic_pressure_wave23 --target x86_64-unknown-linux-gnu --offline -- --nocapture
```

Passed all 6 tests, including the expanded fault corpus and targeted before/after-safepoint reserves, nested guards, slot reuse, graph retention and failed copies. An additional fixed-corpus ASan run passed all 6. Instrumentation covers this runtime crate and test harness compiled for the explicit target; the standard library, foreign runtimes, compiler-generated native programs, and other workspace crates are outside this instrumentation boundary. Fault injection simulates failure at explicit reserve hooks; it is not physical heap exhaustion.

The same fixed-corpus ASan command with `ASAN_OPTIONS=detect_leaks=1` ran all 6 tests successfully, then exited 1 because LeakSanitizer reported a fatal error: it cannot operate under this host's ptrace restrictions. Leak detection is therefore **blocked**, not passed or disabled without disclosure. No host ptrace policy was changed. `cargo +nightly miri --version` reports that cargo-miri is not installed; no Miri result is claimed.

This lane adds direct-runtime evidence only. Existing conformance/source tests remain included in the full suites, but these generated graph operations, reserve hooks, manual root registrations, and generation checks do not run through `.cg` source syntax or compiler lowering. There is no new source graph or concurrency coverage. Source/native equivalence beyond the existing profile, broader ownership/FFI/unwind behavior, independent fuzzing, leak-enabled instrumentation on a compatible host, and the general memory-safety release gate remain open. A finite corpus and zero observed ASan errors do not establish general safety, leak freedom, or a release claim. Stop for review; no merge or follow-on task is included.
