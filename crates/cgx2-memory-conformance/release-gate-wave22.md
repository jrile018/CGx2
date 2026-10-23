# Wave 22 region graph and source unwind evidence

## Scope and replay

`tests/region_graph_wave22.rs` is an independent bounded oracle for two named
regions (one manual and one auto with `trigger_bytes: 4096`, both with
`limit_bytes: 4096`). Its logical nodes own byte expectations, reference and
legacy-link targets, root registration counts, dynamic logical charges, and
live/stale state. It computes reachability without querying collector internals.
After every transition it compares per-region and total committed/live/peak
bytes, allocation failures, collection counts and reclaimed bytes, root
census, checked references, and every live or stale handle. The first observed
error is retained in the model; individual failure results are asserted at
their transition. Closed regions retain report rows but reject `stats` and
region census calls. The same file runs through the normal runtime integration
test and through its `fault-injection` build; conformance runs the latter.

The fixed corpus is `corpus/wave22-region-graph-seeds.csv`; the smallest
human-readable path is `corpus/wave22-region-graph-trace.txt`. A failed assertion
prints the seed and executed operation prefix. Replay all seeds with
`LIBRARY_PATH=/tmp/cgx2-wave22-lib cargo +1.98.1 test -p cgx2-memory-conformance
--test region_graph_wave22 --offline -- --nocapture`, or one with
`CGX2_WAVE22_SEED=0x22` added to that command. The injected mismatch test
alters the oracle's expected byte after allocation and confirms its comparison
panics. This checks that the oracle can detect a divergent runtime value.

Reserve failures at one-based indices 1, 2, and 3 cover the object payload,
reference vector, and slot-table growth on a fresh region. Each is retried
after the one-shot failure, with a rooted object in the unrelated region.
Nested `no_gc(256)` and `no_gc(0)` guards exercise budget exhaustion followed
by retry after the inner guard exits. A failed cross-region reference precedes
source close, while a target-region root survives. Successful managed-to-owned
copy is mutated before promotion and checked for distinct payload storage.
The source language has no graph-link fixture here: reference and legacy-link
operations in this oracle are direct-runtime-only.

## Source/native parity

`crates/cgx2c/tests/memory_release_transfer_reverse.cg` adds the destination
first close order to the existing transfer fixture. The
`memory_release_parity` test compares checked source execution with debug and
release `cgx2c` binaries, including full per-region memory JSON after session
IDs are normalized. Both transfer cases exit successfully with zero remaining
committed/live bytes, no collections, and closed named regions. The new
`memory_release_escape_copy.cg` and existing escape fixture are rejected by
the checked interpreter and debug/release compiler with `CGO003` and
`live handles`. `target/debug/cgx2 run` succeeded for both legal fixtures;
`target/debug/cgx2 memory report FILE --format=json` reported zero final
committed/live bytes and two closed named regions (peak totals 136 and 134
bytes, respectively). `target/debug/cgx2 run` rejected both escape fixtures
with `CGO003` and exit status 1. These cases use the current representable
source profile and do not claim source-level graph links.

## Verification and dynamic instrumentation

Host toolchain: `rustc 1.98.1 (48a229cea 2026-09-01)`; dynamic toolchain:
`rustc 1.101.0-nightly (5c543b0b8 2026-09-29)`. This host has
`/usr/lib/x86_64-linux-gnu/libpython3.14.so.1.0` but no unversioned linker
name. The tests used `LIBRARY_PATH=/tmp/cgx2-wave22-lib`, where
`libpython3.14.so` is a temporary symlink to that installed library.

- `LIBRARY_PATH=/tmp/cgx2-wave22-lib CARGO_INCREMENTAL=0 cargo +1.98.1 test -p cgx2-runtime --offline --quiet`: passed twice, 13 test binaries per pass.
- `LIBRARY_PATH=/tmp/cgx2-wave22-lib CARGO_INCREMENTAL=0 cargo +1.98.1 test -p cgx2-memory-conformance --offline --quiet`: passed twice, 21 test binaries per pass.
- `LIBRARY_PATH=/tmp/cgx2-wave22-lib CARGO_INCREMENTAL=0 cargo +1.98.1 test -p cgx2-runtime --features fault-injection --offline --quiet`: passed.
- `LIBRARY_PATH=/tmp/cgx2-wave22-lib CARGO_INCREMENTAL=0 cargo +1.98.1 test -p cgx2c --test memory_release_parity --offline --quiet`: passed, 3 tests covering checked source and debug/release native outputs.
- `cargo +1.98.1 fmt --all --check` and owned `clippy -D warnings` for `cgx2-runtime` plus `cgx2-memory-conformance --all-targets`, and `cgx2c --test memory_release_parity`: passed.
- `RUSTUP_TOOLCHAIN=nightly RUSTFLAGS=-Zsanitizer=address ASAN_OPTIONS=detect_leaks=0 CARGO_INCREMENTAL=0 cargo test -p cgx2-runtime --test region_graph_wave22 --target x86_64-unknown-linux-gnu --offline --quiet`: AddressSanitizer passed all 3 normal-feature tests. Instrumentation covers the test and runtime code compiled by this command, not the unsanitized standard library, foreign runtimes, or generated `cgx2c` binaries.

The same AddressSanitizer command with `ASAN_OPTIONS=detect_leaks=1` completed
the tests but LeakSanitizer then exited with a fatal error: this host's
`ptrace_scope` disallows the access it needs. Leak freedom remains open.
The bounded corpus and native parity do not establish memory safety for all
supported source programs, arbitrary typed layouts, or foreign memory.
