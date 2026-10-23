# Wave 21: cross-region transfer and alias assurance

## Boundary and replay

The source-representable transfer is `memory.copy_to_owned(original)` followed by
`memory.promote_owned(destination, copied)`. It creates separate byte storage while
leaving `original` valid. The source fixture `memory_release_transfer.cg` runs this
path in a manual source and auto destination, closes the source first, and uses the
destination afterward. `memory_release_escape.cg` is rejected at the close with
`CGO003` and `region 'source' has live handles`.

The direct-runtime replay in `cgx2-runtime/tests/transfer_replay.rs` records every
operation's region index, handle generation, owned-value count, root registrations,
committed/live bytes, collection count, first error, and handle validity. Replaying
the same sequence twice must yield identical steps. It runs both close orders with
an owned four-byte source, two roots on its promoted managed handle, a distinct
mutable owned copy promoted to another region, a forbidden cross-region edge, a
rooted close failure, and successful cleanup. After the rejected edge, both handles,
all three roots, and both regions' charges remain intact. After the first close,
only that region's handle is invalid; the other is usable. Closing an already closed
region returns `InvalidRegion` without changing the report.

`cgx2-memory-conformance/corpus/wave21-region-transfer.csv` selects four
auto/manual mode and close-order combinations. Its test uses a checked reference
slot for a separate direct-runtime graph operation: `set_reference(source, 0,
Some(destination))` returns `CrossRegionEdge` and leaves the slot empty, root census
and report unchanged. The successful copy has different source and destination
byte pointers. These graph operations have no claimed `.cg` spelling.

The fault replay validates the source handle before a reserve, injects a
`ManagedToOwned` reserve failure after validation, and injects a `SlotTable` failure
before destination promotion commits. It verifies destination committed/live bytes
and roots are unchanged, the original remains readable, and the failed promotion
returns its owned value for retry. A nested `no_gc(4096)` pair surrounds failure
and retry; collection count remains zero throughout, and the outer guard still
rejects collection after the inner guard exits. A close attempted with a rooted
handle returns `RegionHasRoots`; unroot, close, and root census cleanup succeed.
There is no runtime host-reserve or fault-injection point inside `close_region`, so
the close-path fault boundary is the exposed rooted-close error.

## Source and native observations

With the host linker workaround described below, `cgx2 run` succeeds silently for
`memory_release_transfer.cg`. `cgx2 memory report ... --format=json` reports three
regions: the default region open at zero bytes; source manual and destination auto
both closed at zero committed/live bytes, peak 68 bytes each, and zero collections.
`cgx2 run` rejects `memory_release_escape.cg` with `CGO003` at span `157..177`.
`memory_release_parity.rs` compares the complete source interpreter region report
(apart from process-local session IDs) against both debug and release `cgx2c`
output; native
diagnostics reject the same escape in both profiles. Successful source/native
memory reports have no root-census field, so root-count evidence is from direct
runtime tests and the existing failure-report parity tests.

## Verification and dynamic instrumentation

Toolchain: Rust 1.98.1 for normal builds, Clippy, and rustfmt; Rust 1.101.0-nightly
(`5c543b0b8`, 2026-09-29) for AddressSanitizer. This host has
`/usr/lib/x86_64-linux-gnu/libpython3.14.so.1.0` but lacks the unversioned linker
name. Normal source and test commands used a temporary
`/tmp/cgx2-wave21-lib/libpython3.14.so` symlink and
`LIBRARY_PATH=/tmp/cgx2-wave21-lib`; no project toolchain file was changed.

- `LIBRARY_PATH=/tmp/cgx2-wave21-lib cargo test -p cgx2-runtime --features fault-injection` — pass, twice.
- `LIBRARY_PATH=/tmp/cgx2-wave21-lib cargo test -p cgx2-memory-conformance` — pass, twice.
- `LIBRARY_PATH=/tmp/cgx2-wave21-lib cargo test -p cgx2c --test memory_release_parity` — pass; the transfer test builds and runs debug and release binaries and checks debug and release static rejection.
- `rustfmt +1.98.1 --check --edition 2021` on the four changed Rust files — pass.
- `LIBRARY_PATH=/tmp/cgx2-wave21-lib cargo +1.98.1 clippy -p cgx2-runtime -p cgx2-memory-conformance --all-targets --features cgx2-runtime/fault-injection -- -D warnings` — pass.
- `LIBRARY_PATH=/tmp/cgx2-wave21-lib cargo +1.98.1 clippy -p cgx2c --test memory_release_parity -- -D warnings` — pass.

For the source/native path, `cgx2c emit-rust` generated
`/tmp/cgx2-wave21-transfer.rs` from the legal transfer fixture. That exact file
was used as `src/main.rs` in a temporary Cargo package depending on this worktree's
`cgx2-runtime`. The dynamic run was:

```sh
ASAN_OPTIONS=detect_leaks=0:halt_on_error=1 \
RUSTFLAGS='-Zsanitizer=address' \
CARGO_TARGET_DIR=/tmp/cgx2-wave21-asan-native-target \
cargo +nightly run --offline --manifest-path /tmp/cgx2-wave21-asan-native/Cargo.toml
```

Result: exit 0, no AddressSanitizer finding. Generated source and the path
dependency runtime were compiled with the sanitizer; the prebuilt standard
library and external Python/foreign allocators were outside that instrumentation
boundary. A separate AddressSanitizer run of `transfer_replay` passed both tests
with `detect_leaks=0`. With `detect_leaks=1`, LeakSanitizer exited 1 after both
tests passed because this environment blocks its ptrace-based leak inspection.
Leak freedom is therefore unverified by this tool here.

This finite corpus, successful cleanup census, and sanitizer pass support the
specific cases above. Foreign allocation, parallel sharing, and general memory
safety remain release gates.
