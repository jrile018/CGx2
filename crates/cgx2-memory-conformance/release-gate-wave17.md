# Wave 17 direct-runtime root census evidence

## Boundary and result

`Session::root_census()` and `Session::root_census_region(region: RegionId)` read live slot tables directly. Each returns `RootCensus { rooted_handles, root_registrations }`; repeated roots of one handle contribute one handle and multiple registrations. Both methods allocate nothing, take `&self`, and use checked addition with `MemoryError::RootCensusOverflow`. The region method returns `MemoryError::InvalidRegion` for a foreign or closed region. Closed regions do not contribute to the session total. Existing `root`, `unroot`, collection, and close behavior is unchanged.

`crates/cgx2-runtime/tests/root_census.rs` covers duplicate roots, partial/full/failed unroot, same-region cyclic references, rooted collection, region-close failure and success, stale handles, and foreign/closed region IDs. The runtime unit test forces a registration-sum overflow in an internal slot to verify the defined error. `crates/cgx2-memory-conformance/tests/root_census_cleanup.rs` checks nested `no_gc`, injected host reserve failure, read-only census calls against `MemoryReport` and fault counters, and source-like two-binding cleanup. This test models cleanup through direct runtime calls; it does not execute interpreter or native failure reporting.

The existing `automatic_collection_model.rs` corpus keeps `SEEDS = 1024`, `RANDOM_STEPS = 96`, and `STEPS_PER_SEED = 125` (128,000 scheduled transitions). The census tests are deterministic and have no random step budget.

## Commands run in this worktree

| Command | Result |
| --- | --- |
| `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline` | Passed twice after the final code edit: 106 tests in 28 test binaries per run, zero failures. |
| `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --package cgx2-runtime --package cgx2-memory-conformance -- --check` | Passed. |
| `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo clippy -p cgx2-runtime -p cgx2-memory-conformance --all-targets --offline -- -D warnings` | Passed. |
| `RUSTUP_TOOLCHAIN=nightly RUSTFLAGS=-Zsanitizer=address ASAN_OPTIONS=detect_leaks=0 cargo test -p cgx2-runtime --test root_census --offline --target x86_64-unknown-linux-gnu` | Passed: 2 tests. |
| `RUSTUP_TOOLCHAIN=nightly LIBRARY_PATH=/tmp/cgx2-python-lib RUSTFLAGS=-Zsanitizer=address ASAN_OPTIONS=detect_leaks=0 cargo test -p cgx2-memory-conformance --test root_census_cleanup --offline --target x86_64-unknown-linux-gnu` | Passed: 1 test. |
| `RUSTUP_TOOLCHAIN=nightly RUSTFLAGS=-Zsanitizer=leak cargo test -p cgx2-runtime --test root_census --offline --target x86_64-unknown-linux-gnu` | Both tests printed `ok`, then LeakSanitizer exited 23 because the host blocks its `ptrace` requirement. No leak result is claimed. |

Miri is not installed in the available nightly toolchain. The first unconfigured test attempt could not link `-lpython3.14`; the existing `/tmp/cgx2-python-lib` search path resolved that host setup issue for the recorded passing runs.

## Remaining release gaps

A zero census is a necessary cleanup condition, not proof of no leaks or invalid memory access. No interpreter or emitted-native consumer uses this API yet, so source/native failure and root parity remain unverified. Foreign Python/C/LibTorch lifetimes, shared-session concurrency, a successful LeakSanitizer run, Miri, general memory safety proof, and independent review remain open. AddressSanitizer covered only the two named test targets.
