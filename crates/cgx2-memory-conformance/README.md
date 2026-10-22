# Deterministic memory corpus

Wave 14's bounded graph and reserve audit, including the explicit release-gate
matrix and replay limits, is in [release-gate-wave14.md](release-gate-wave14.md).

## Wave 12 checked reference graph model

`tests/graph.rs` runs 128 deterministic seeds (0 through 127), 80 transitions
per seed, using the v1–v5 wrapping LCG (`state = state *
6364136223846793005 + 1`, upper 32 bits for each choice). It compares an
independent graph model with `Session` after every transition: live and stale
handles, two checked slots per object, root counts, region-local collection,
manual and auto modes, object charges, failure counts, and per-region memory
reports. Operations include allocation, root/unroot, set/clear, collection,
out-of-range slot, oversized shape, and cross-region edge failures. The exact
model and failure atomicity rules are in [graph-model.md](graph-model.md).
Runtime `tests/objects.rs` adds directed cycle, shared-child, legacy-link,
region-close, auto threshold, and nested `no_gc` fault cases. The v1–v5
conformance code and seed manifests remain unchanged. These finite tests do
not cover host allocator fault injection, Miri, sanitizers, source-level escape
analysis, foreign handles, or universal memory safety; the release audit gate
remains open.

Replay v1 with `cargo run -p cgx2-memory-conformance --offline -- --seed 42`. Replay v2 with `cargo run -p cgx2-memory-conformance --offline -- --seed 42 --model-version 2`. The command prints the `.cg` source, model counters, and runtime report. `cargo test -p cgx2-memory-conformance --offline` checks both manifests and failure fixtures. A failing test includes the model version, seed, source, expected counters, and observed error or report.

Model version 1 uses a wrapping 64-bit linear congruential generator: `state = state * 6364136223846793005 + 1`; each choice uses the upper 32 bits modulo the number of alternatives. The CSV records model version and 128 fixed seeds. Generated cases have two explicit regions, at most 48 statements, 4096 source bytes, and payloads at most 32 bytes. Both region modes, owned moves, managed roots and drops, explicit collection, region close, and nested guards are exercised. The reference model uses the Wave 5 slot charge of 64 bytes plus payload length and the owned guard charge of 32 bytes plus payload length. Auto regions never reach their 1 MiB threshold in this generated corpus, so their automatic safepoint behavior needs the existing CLI/runtime tests.

The source API does not return a report on `AllocationError`; the direct runtime companion test checks counters after a nested guard failure. Pause durations and histogram buckets are deliberately excluded from deterministic comparisons. This finite corpus cannot establish absence of memory bugs, borrow escape, or raw pointer safety. Broader fuzzing and runtime safety audit remain release gates.

## Model version 2

`corpus/seeds-v2.csv` fixes 256 seeds, 0 through 255. V2 keeps the same wrapping 64-bit LCG and upper-32-bit modulo choice rule as v1. It generates two regions with all four auto/manual mode pairs, owned move and borrow, promotion into each region, managed-to-owned copy, nested `no_gc` guards, roots and drops, explicit region-local collection, and close. Generated successful scripts have at most 48 statements, 4096 UTF-8 bytes, and 32-byte payloads. The model starts from empty counters: managed storage charges 64 slot bytes plus payload, owned creation/copy charges 32 owned metadata bytes plus payload to active guards, and promotion charges only new 64-byte slot metadata to active guards while adding slot plus payload to the destination region. Its expected report is computed from its own transitions, then compared with the source runtime report. Zero-length payloads are included.

Each v2 seed also generates three statically valid dynamic-failure scripts through `generate_v2_failures`: promotion under a 63-byte `no_gc` budget, promotion with a destination limit one byte too small, and managed-to-owned copy under a 31-byte budget. All must yield `CGR013` with a source span and no successful report. The direct `Session` tests inspect preserved owned bytes, source handles, allocation-failure counters, and zero collection after failures. The v1 and v2 fixed corpora each run twice; generated source, model expectations, and deterministic runtime counters must be byte-identical. Wall-clock pause measurements are intentionally omitted from this equality check.

Invalid source fixtures require exact `CGO001` use-after-move/stale-binding, `CGO002` borrow conflict, `CGO003` close-with-live-handle, and `CGO004` collection-in-guard families with spans. Safe source cannot express a raw stale handle after collection or a cross-region `Session::link`; those are tested through direct runtime handles. The runtime is a single-threaded byte-object model. This corpus does not cover concurrency, arbitrary object layouts, foreign memory, sanitizer certification, randomized mutation fuzzing, or a universal memory-safety proof. The first-release memory audit and fuzz gate remain open.

## Model version 3: direct-runtime state machine

`corpus/seeds-v3.csv` fixes seeds 0 through 999. Replay one with `cargo run -p cgx2-memory-conformance --offline -- --seed 42 --model-version 3`. Version 3 keeps the v1/v2 wrapping 64-bit LCG (`state = state * 6364136223846793005 + 1`), using the upper 32 bits modulo each choice count. It is a separate model over the public `cgx2-runtime::Session` API; source-script v1/v2 behavior remains unchanged. A change to the generator, PRNG, or transition rules requires a new model version and manifest.

Each trace has at most 96 operations, three regions including the default, two nested `no_gc` guards, 96 handles/owned values, and 32-byte payloads. The fixed prefix exercises a manual region, an auto region, cross-region link rejection, wrong-session handle/region rejection, zero-length allocation, nested budget exhaustion, forbidden collection, near-limit failure, stale handles, promotion, and independent managed-to-owned copying. The remaining operations choose allocation, conversion, rooting, links, reads, collection, close, and guard transitions. A request for an owned value or handle before one exists, or for an already consumed owned value, is an explicit `Skipped` transition; the model and runtime still compare unchanged counters after it. Region and guard failures are executed rather than filtered out.

After every step, the runner compares the expected operation result, all deterministic region and total counters, `live <= committed <= peak`, region limits, pause sample counts, each retained handle's bytes or stale error, and every owned buffer's bytes and ownership state. The reference model tracks roots and graph reachability separately from the runtime. Collection is forbidden throughout guards; only explicit collection changes a manual region's count. Auto collection is modeled at the next allocation safepoint after its 256-byte trigger. Pause durations and histogram buckets are excluded from equality because they depend on wall time.

On a mismatch, the CLI prints the seed, original numbered operation trace, and a minimized trace. The shrinker replays candidates in fresh sessions, deletes spans, then reduces payload lengths to zero or one while retaining a per-step mismatch. The `OvercountFirstAllocation` fault test proves deterministic replay and shrinking without requiring a real runtime defect. The fixed 1,000-seed manifest runs twice in tests; successful runs compare the same deterministic counters each time.

This is finite safe-Rust API testing, not proof of universal memory safety. It omits source-level borrow/escape rules, foreign handles and memory, arbitrary object layouts, concurrent sessions, host allocator failures, sanitizer coverage, and cancellation. Miri was not installed for this Wave 8 run. The release hard memory gate remains open.

## Model version 4: memory assurance audit

`corpus/seeds-v4.csv` fixes seeds 0 through 1999. V4 reuses the v3 public-`Session` transition model and per-step comparison but freezes a separate trace generator. Its wrapping 64-bit LCG uses `state = state * 6364136223846793005 + 1`, then the upper 32 bits modulo each choice count. The seed-42 formatted trace has FNV-1a-64 hash `fee821be2efde9c0`; changing the PRNG, operations, transition rules, or trace serialization requires a new model version. V1 (128 seeds), v2 (256), and v3 (1,000) replay remain in their existing tests, including `--seed N --model-version 3`.

V4 traces have at most 128 operations, four total regions, 128 retained handles and owned values, 64-byte payloads, and two nested guards. A fixed prefix makes the named adversarial paths mandatory before a bounded random tail. After each operation, the independent model checks the result, deterministic region and total counters, pause sample counts, all retained handles, and all owned values. The observer records operation, result/error family, skipped operation, and named adversarial coverage. It normalizes successful byte reads to `Ok(bytes)` in summaries; exact payload bytes are still compared during replay. Failed operations and explicit skips are included in the step count. The v3 shrinker preserves a per-step mismatch and replays each candidate in a fresh session; a fault-injected v4 test checks deterministic minimization.

Run one trace with `cargo run -q -p cgx2-memory-conformance --offline -- --seed 42 --model-version 4`. Run the fixed set with `cargo run -q -p cgx2-memory-conformance --offline -- --model-version 4 --corpus-count 2000`; run the extended set with `--corpus-count 10000`. The corpus command emits deterministic JSON coverage. `audit-v4.json` records both fixed runs, the extended run, exact counters, CPU and wall time, skipped operations, observed failures, and untested dimensions. The fixed 2,000-seed set passed twice with byte-identical summaries; the extended 10,000-seed set passed once. No runtime mismatch was found or repaired in this audit.

The audit covers byte objects and explicit same-region edges through the safe Rust API. It does not cover generated typed layouts, source-level borrow/escape rules, shared-session concurrency, foreign Python/PyTorch/device allocation, or host allocation fault injection. Foreign memory is never charged to a CGx2 region by this model. Miri was unavailable on installed toolchains, and no sanitizer run was performed. Finite passing traces do not close the first-release memory hard gate.

Wave 10 keeps the v4 trace serialization, generator, transition model, and 2,000-seed manifest frozen. `tests/failure_regressions.rs` replays four fixed seeds (0, 1, 42, 255) through each of two new stateful direct-runtime paths: impossible requests before an auto trigger, and reclaimable capacity pressure below that trigger followed by a rooted failure. Each step checks deterministic counters and handle validity; these traces are separate from the v4 coverage JSON because changing that model would require a new model version. They do not simulate host allocator exhaustion or establish a universal safety proof.

## Model version 5: allocation ordering and guard assurance

V5 is a separate transition model in `src/v5.rs`; v1–v4 traces and the Wave 10 regression tests remain unchanged. Its fixed prefix exercises intrinsically impossible allocations before an auto safepoint, auto collection at a threshold, reclaim below a threshold when capacity is insufficient, rooted failure after collection, manual region failure, nested `no_gc` budget exhaustion and forbidden collection, failed then successful owned promotion, an independent managed-to-owned copy, a rooted cycle, stale handles after collection and slot reuse, and region close. A seeded bounded tail adds further allocation, collection, copy, read, and close interleavings. Each step compares the result, region and total live/committed/peak/failure/collection counters, pause sample counts, owned payloads, and validity and bytes of every retained handle. `tests/v5.rs` also checks guard unwind after a panic and the fault-injected deletion reducer.

Replay a seed with `cargo run -q -p cgx2-memory-conformance --offline -- --model-version 5 --seed 42`; add `--reduce` to print its minimized mismatch trace (or the original trace when it passes). The CI-sized test covers 64 seeds. Generate a repeated fixed audit with `--model-version 5 --corpus-count 2000 --repeats 2`; the opt-in extended audit uses `--corpus-count 10000 --repeats 1`. The machine-readable `audit-v5.json` records exact seed ranges, operation and error counts, skips, repeated-run agreement, and dynamic-check availability. This run found no runtime mismatch or repair: 2,000 seeds and 188,000 verified steps agreed on two runs; 10,000 seeds and 940,000 verified steps passed once. Miri was unavailable and no sanitizer was configured. The finite corpus covers only the public single-threaded byte-object API; typed layouts, source borrow and escape rules, foreign allocators, concurrency, and host allocator exhaustion remain outside this audit and the release memory gate remains open.
## Wave 13 reserve-failure and graph assurance

The conformance crate enables the runtime's `fault-injection` feature only for
its direct-runtime tests. `tests/fault_injection.rs` fails each reachable
fallible reserve in region creation, managed bytes, object payload and checked
references, new slots, owned bytes, managed-to-owned copy, promotion, and
legacy edge growth. Slot-table cases fill the existing capacity before
arming, so the selected reserve requires host growth. The tests check the
original error family, unchanged graph and
root behavior, report counters, retained `OwnedBytes` after failed promotion,
and zero collections within nested `no_gc` guards. A subsequent allocation
checks that the one-shot failure did not commit a slot.

`tests/graph_failure_model.rs` uses a logical-node reachability oracle with
64 fixed seeds (`0..64`), 96 scheduled steps and 100 random operations per
seed, and the v1–v5
wrapping LCG (`state * 6364136223846793005 + 1`, upper 32 bits modulo choice
count). Its fixed prefix forces stale-handle reuse, overwritten and cleared
edges, and an unreachable cycle. Each seed then checks manual/auto regions,
root counts, references, two forced reserve failures inside nested guards,
success after failure, cross-region rejection, auto safepoints, collection
suppression, region closure, every retained live or stale handle, and report
counters after each operation. The v1–v5 corpus generators and seed manifests
are unchanged. See `audit-wave13.json` and [graph-model.md](graph-model.md).
