# Wave 16 memory release evidence

**Decision: memory release gate remains open.** This packet covers the single-threaded direct
`cgx2_runtime::Session` API and its feature-gated host-reserve fault injector. It is finite
conformance evidence, not a proof for all safe CGx2 programs. No runtime defect was demonstrated
by this lane, so the production runtime and public API were left unchanged.

## Implemented boundaries and corpus

`Session::new` creates a default auto region with `trigger_bytes = 1 << 20` and
`limit_bytes = 64 << 20`. `create_region(mode, limit_bytes)` uses the supplied values.
`alloc_bytes` and `alloc_object` are the automatic safepoints: outside `no_gc`, they collect
only their destination auto region when committed bytes are already at least `trigger_bytes`
or the next charge cannot fit the region limit. `alloc_bytes` reaches that safepoint before its
host byte reserve; `alloc_object` reserves payload and checked-reference vectors first, then
rechecks capacity and reaches the safepoint before slot commitment. A collection that cannot
make room may therefore precede a `RegionLimit` or later host-reserve failure. `promote_owned`,
`link` edge growth, `set_reference`, `root`, `unroot`, and `create_region` are not automatic
safepoints. Manual regions collect only on an explicit `collect`; `close_region` releases storage
without incrementing collection count. Any active `no_gc` guard forbids explicit and automatic
collection across the session. Nested budgets each charge successful allocations, including
region bookkeeping (128 bytes), and return `BudgetExceeded` before commitment when exhausted.

`tests/automatic_collection_model.rs` replays seeds **0..1024**, each with 23 fixed, 96
generated, and 6 close operations: **125 transitions per seed, 128,000 per run**. Choices use
wrapping `state = state * 6364136223846793005 + 1` and upper 32 bits modulo the choice count.
The fixed prefix forces a rooted graph and cycle across the first threshold, then reclaims a
cleared child edge and unreachable cycle at the next safepoint. The tail covers auto/manual
regions, copies, promotions, cross-region edge rejection, nested zero-budget guards, stale
handles, manual collection, and close attempts. On mismatch it reports seed/step and deletion-
minimizes the trace. The model uses logical node IDs and independent reachability, not runtime
slot indices. After **every** transition it compares liveness, references, per-region and total
committed/live/peak/collected bytes, allocation failures, collection and pause-sample counts.
It checks root counts by draining and restoring roots through the public API without allocating
or collecting.

Bound per run: **30 seconds**, **160 logical nodes**, **125 operations per seed**, two named
regions with **23,200 logical committed bytes each / 46,400 combined**. The auto trigger is
**435 bytes**, three 145-byte fixed-shape objects; a promoted one-byte value charges 65 bytes.
Both release corpus replays passed in 0.31 and 0.34 seconds on this worktree. The bound concerns
CGx2 charged region bytes; process RSS, allocator capacity, and foreign memory were not bounded.

## Failure and cleanup evidence

The full targeted suite includes these adversarial paths:

| Boundary | Checked evidence |
| --- | --- |
| Payload, checked-reference, managed-byte, slot-table, and region-table reserves | `fault_injection.rs`, `automatic_collection_faults.rs`, and `release_faults.rs` inject one-shot `HostAllocation`, compare unchanged prior graph/bytes and failure counters, then retry. |
| Edge growth and managed-to-owned copy | `fault_injection.rs` and `automatic_collection_faults.rs` check atomicity, retained roots/edges, and a later valid operation. |
| Promotion | `automatic_collection_faults.rs` checks failed promotion returns the unchanged owned value and does not create an automatic safepoint. |
| Collection boundary | `release_faults.rs` arms a reserve failure across explicit fixed-point collection, verifies zero host-reserve attempts during collection, then observes the pending fault at the next allocation. `automatic_collection_faults.rs` distinguishes pre-safepoint object-reserve failure from post-safepoint byte-reserve failure. |
| Manual and nested `no_gc` failure | `fault_injection.rs`, `automatic_collection_faults.rs`, and `release_cleanup.rs` check zero implicit collections, budget and region-limit precedence, live roots, and successful retry. |
| Cancellation and returned error | New `release_cleanup.rs` unwinds two nested guards after host and budget failures, then propagates an allocation error with `?`. Counts remain unchanged inside the guards; after cleanup the next auto safepoint collects only its region. Manual close rejects live roots; explicit collection and close leave the other region intact. |

These tests found no new failure trace requiring a runtime repair. The existing injector targets
fallible reserve sites; it does not simulate arbitrary host allocator or process failure during
the allocation-free fixed-point collector.

## Verification on this worktree

All standard checks used Rust **1.98.1**, `--offline`, and
`LIBRARY_PATH=/tmp/cgx2-python-lib`, a temporary link to the installed
`/usr/lib/x86_64-linux-gnu/libpython3.14.so.1.0` needed by the conformance crate's CLI
dependency. The link is outside the committed tree.

| Command | Result |
| --- | --- |
| `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline` | Passed. |
| `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-memory-conformance --test automatic_collection_model --offline -- --nocapture` | Passed twice; 128,000 transitions each run. |
| `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --package cgx2-runtime --package cgx2-memory-conformance -- --check` | Passed. |
| `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo clippy -p cgx2-runtime -p cgx2-memory-conformance --all-targets --offline -- -D warnings` | Passed. |
| `RUSTUP_TOOLCHAIN=nightly RUSTFLAGS=-Zsanitizer=address ASAN_OPTIONS=detect_leaks=0 cargo test -p cgx2-runtime --offline --target x86_64-unknown-linux-gnu` | AddressSanitizer passed. |
| `RUSTUP_TOOLCHAIN=nightly RUSTFLAGS=-Zsanitizer=address ASAN_OPTIONS=detect_leaks=0 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-memory-conformance --offline --target x86_64-unknown-linux-gnu --test automatic_collection_model --test release_cleanup` | AddressSanitizer passed. |

The first AddressSanitizer attempt with leak detection enabled ended in LeakSanitizer's
`ptrace` fatal error after a test passed; this host cannot run LeakSanitizer under that setting.
The successful runs disabled **leak detection only**. Miri is **unrun**: `cargo +nightly miri
--version` reports `cargo-miri` is not installed for the nightly toolchain. Nightly rustc was
`1.101.0-nightly (5c543b0b8 2026-09-29)` and emitted a pre-existing `fetch_update`
deprecation warning. No Valgrind executable was found.

## Unrun release gates

| Gate | Status and reason |
| --- | --- |
| Source interpreter and emitted native failure/root parity | **Unrun here.** This lane owns only direct runtime/conformance paths; the CLI/emitter lane owns the parity check. |
| Foreign Python, C, and real LibTorch allocator/handle lifetime | **Unrun here.** Foreign adapters are owned by another lane; this corpus models only CGx2 region charges. |
| Thread/concurrency safety | **Unrun.** `Session` is a single-threaded execution session; no shared-session claim follows from these tests. |
| Miri and LeakSanitizer | **Unrun** for the installation and host `ptrace` reasons above. AddressSanitizer covered only the stated test targets. |
| General safe-program memory proof and independent safety review | **Open.** Finite deterministic traces and sanitizer tests cannot establish universal absence of memory errors. |
