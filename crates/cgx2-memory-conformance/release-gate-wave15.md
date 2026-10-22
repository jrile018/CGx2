# Wave 15 automatic collection assurance

The first-release memory correctness gate remains **open**. This lane tests the
single-threaded direct `cgx2_runtime::Session` API and its existing
`fault-injection` feature. It does not establish universal memory safety.

## Direct-runtime evidence

`tests/automatic_collection_model.rs` runs seeds `0..64` using the wrapping
LCG `state = state * 6364136223846793005 + 1` and upper 32 bits modulo each
choice. Each trace has 23 fixed operations, 96 generated operations and 6
closing operations (125 actual, 132 allowed), at most 160 logical nodes, a
30-second test bound and two named regions. The auto region has
`trigger_bytes = 435` (three 145-byte objects); each named region has a
23,200-byte limit. Objects have a one-byte payload and two checked reference
slots; promoted owned values have a one-byte payload and a 65-byte region
charge. The model tracks root counts, checked references and reachability by
logical node ID, independently of runtime slots. Its fixed prefix makes the
first threshold collection retain a rooted parent/child/cycle, then makes the
next reclaim the cleared child edge and unreachable cycle. It also exercises
stale handles, owned promotion and copy, explicit manual collection, and
nested zero-budget `no_gc`. Every transition compares live and stale handles,
stored references, per-region counters, pause sample counts and total counters.
On mismatch it prints the seed, failing step and a deletion-minimized trace.
Pause durations are excluded from deterministic comparisons.

`tests/automatic_collection_faults.rs` exercises positive nested `no_gc`
budgets across two auto regions and a manual region. The first auto allocation
after both guards exit collects only its destination region; the second does
the same for the other region. The manual collection count remains zero.
Object payload and reference reservation faults occur before an auto
safepoint, so neither increments its collection count. A one-byte
`alloc_bytes` fault occurs after a threshold safepoint, so it increments the
destination collection count once while leaving its rooted graph and
committed bytes intact. Failed edge growth leaves the graph unchanged; the
next allocation reclaims the unlinked child, and a successful replacement
link survives the following collection. Explicit final collection releases
the graph exactly once. The existing `release_faults.rs` and
`fault_injection.rs` cases were rerun for slot growth, promotion, copy and
manual limit precedence. A known manual limit failure returns
`RegionLimit` before an armed host reserve (`host_reserve_attempts() == 0`).

No new public controls were needed: `stats`, `memory_report`, and the existing
feature-gated reserve injection and attempt counters identify the collection
boundary and its host-fault order. This leaves the public `Session` graph API
unchanged.

The frozen v3–v5 model treats `promote_owned` as **not** an auto collection
safepoint. At an auto region limit, promotion returns the unchanged owned
value with `RegionLimit`; an explicit `collect` can then make room. A trial
change that collected on promotion failed the frozen v3 seed 62 report model,
so it was not retained. Source and native parity must settle this semantic
boundary before claiming a general automatic-collection guarantee.

## Verification on this worktree

- `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline`: passed twice. The temporary linker directory points `libpython3.14.so` at the installed Python 3.14.4 shared library; no Python allocation is charged to this audit.
- `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --package cgx2-runtime --package cgx2-memory-conformance -- --check`: passed.
- `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo clippy -p cgx2-runtime -p cgx2-memory-conformance --all-targets --offline -- -D warnings`: passed.
- `RUSTUP_TOOLCHAIN=nightly RUSTFLAGS='-Zsanitizer=address' cargo test -p cgx2-runtime --offline`: passed. Nightly was `rustc 1.101.0-nightly (5c543b0b8 2026-09-29)`; it emitted one pre-existing deprecation warning for `AtomicU64::fetch_update`.
- `RUSTUP_TOOLCHAIN=nightly RUSTFLAGS='-Zsanitizer=address' LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-memory-conformance --offline --target x86_64-unknown-linux-gnu --test automatic_collection_model --test automatic_collection_faults`: passed. The explicit target keeps proc macros on the host toolchain; an initial attempt without `--target` could not build sanitized proc macros.
- `RUSTUP_TOOLCHAIN=nightly cargo miri --version`: unavailable because the installed nightly has no `miri` component. Miri tests are unrun.

## Dependency gates

| Gate | Status |
| --- | --- |
| Direct runtime graph, auto safepoints and reserve faults | Passing finite corpus above. Sanitized new cases passed; this is evidence, not a proof. |
| `.cg` interpreter and native parity | Open. The compiler lane must compare source/native error classes, partial reports, graph liveness and the promotion boundary with this direct-runtime behavior. |
| Foreign lifetimes and memory | Open. LibTorch/Python tensors, aliases, handle release and external allocations are outside these region charges; the foreign lane owns that audit. |
| Thread safety | Open for any shared-session claim. `Session` is single-threaded; these tests do not race shared regions. |
| Dynamic tools and safety review | AddressSanitizer passed on the stated paths; Miri was unavailable. An independent safety review and broader dynamic coverage remain open. |
