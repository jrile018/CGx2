# Wave 19 direct-runtime unwind assurance

The bounded replay test is `tests/unwind_replay.rs`; its four fixed rows are in
`corpus/wave19-unwind-traces.csv`. Set `CGX2_REPLAY_SEED` to one of the hex seeds
and run the test below to replay one row. The seed is a stable trace identifier,
not a pseudorandom generator input.

| Seed | Host exit | Guarded injected reserve | Exact operation trace |
| --- | --- | --- | --- |
| `0x19a001` | early return | `1` (`ObjectPayload`) | A |
| `0x19a002` | caught panic | `1` (`ObjectPayload`) | A |
| `0x19a003` | early return | `2` (`ObjectReferences`) | A |
| `0x19a004` | caught panic | `2` (`ObjectReferences`) | A |

Trace A: create a session; fail the first `RegionTable` reserve during region
creation; verify report and root census unchanged; create a manual 4096-byte
region and exit the host frame; allocate an object and byte child; register two
owner roots and one child root; set the checked owner-to-child reference; remove
the child root; enter outer and inner `no_gc(200)` guards; inject the row's
reserve failure into `alloc_object(region, 1, 1)`; separately observe an
`unroot(child)` cleanup failure (`NotRooted`) while retaining the original
`HostAllocation` failure; verify no new root, byte charge, or collection; then
allocate and root a one-byte transient inside the inner guard and exit the host
frame. The inner guard drops on both exit paths. While the outer guard remains,
collection returns `CollectionForbidden`, and `alloc_bytes(region, 200)` returns
`BudgetExceeded` without changing committed bytes or collection count. After
the outer guard drops, a failed close preserves all roots. Explicit collection
keeps the graph and transient alive. Releasing the transient root and collecting
reclaims it. Releasing one duplicate owner root keeps the graph alive; releasing
the last and collecting reclaims owner and child. Close runs inside a host exit
frame and leaves a closed, empty region. Every checkpoint tests root census,
region/report accounting, collection count, and live/stale handles.

`NoGcGuard` uses `Drop` to pop one nested budget on unwind. Root registrations
and region closure are explicit runtime operations; host stack unwinding does
not automatically unroot handles or close regions. The replay tests that
contract without invalidating retained handles.

## Commands and results

- Host: Linux x86-64. Baseline `rustc 1.98.1 (48a229cea 2026-09-01)`;
  dynamic tool `rustc 1.101.0-nightly (5c543b0b8 2026-09-29)`.
- Test dependency linker setup: `/usr/lib/x86_64-linux-gnu/libpython3.14.so.1.0`
  exists, but the unversioned `libpython3.14.so` is absent. A temporary link at
  `/tmp/cgx2-wave19-lib/libpython3.14.so` supplies it through `LIBRARY_PATH`.
  No repository file supplies or modifies that library.
- `LIBRARY_PATH=/tmp/cgx2-wave19-lib cargo test --offline -p cgx2-runtime -p cgx2-memory-conformance`
  ran twice: 30 suites, 111 passed, 0 failed, 0 ignored in each run.
- For each `CGX2_REPLAY_SEED=0x19a001`, `0x19a002`, `0x19a003`, and `0x19a004`:
  `CGX2_REPLAY_SEED=<seed> cargo test --offline -q -p cgx2-runtime --features fault-injection --test unwind_replay -- --exact fixed_host_unwind_corpus`
  passed (1 test per seed).
- `cargo +1.98.1 fmt --all --check`: passed.
- `LIBRARY_PATH=/tmp/cgx2-wave19-lib cargo +1.98.1 clippy --offline -p cgx2-runtime -p cgx2-memory-conformance --all-targets -- -D warnings`:
  passed.
- AddressSanitizer: `CARGO_TARGET_DIR=/tmp/cgx2-wave19-asan RUSTFLAGS='-Zsanitizer=address' ASAN_OPTIONS=detect_leaks=0 cargo +nightly test --offline -p cgx2-runtime --features fault-injection --test unwind_replay`
  passed (1 test, all four rows). Leak detection was disabled because this
  host's `ptrace_scope` prevents LeakSanitizer startup. A first attempt without
  `ASAN_OPTIONS=detect_leaks=0` failed during build-script startup for that
  reason. An attempt to instrument `cgx2-memory-conformance` failed because
  sanitizer-enabled proc macro crates are unsupported by this toolchain; the
  dependency-free runtime test is the dynamic-tool scope.

This evidence concerns a single-threaded direct-runtime corpus. It does not
establish source/native lowering, foreign adapter, or parallel-memory safety,
and does not prove memory safety for untested states. AddressSanitizer used a
prebuilt standard library and did not run leak detection.
