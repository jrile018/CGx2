# Wave 18 direct runtime memory state assurance

## Scope and bounds

`tests/faulted_state_model.rs` uses a separate logical-ID reachability model. The
fixture `corpus/wave18-state-seeds.csv` fixes seeds `0x1801`, `0x1802`, and
`0x1803` at 64 random steps each. Each replay exercises two regions (manual and
auto with `trigger_bytes: 0`, each limited to 4096 bytes), plus the unused
default region. At most 14 nodes are live at once and each object has two
checked reference slots; at most 66 logical nodes are created per replay.

Modeled operations are one-byte managed allocation, one-byte/two-reference
object allocation, one-byte owned allocation and promotion, duplicate root
registration and release, checked reference set/clear, explicit and automatic
collection, nested `no_gc(4096)`, and region close. After each modeled step,
the test compares region and global `rooted_handles`/`root_registrations`,
handle liveness, checked references, all stable `RegionStats` fields, and
memory-report totals. The owned lifecycle test additionally checks after owned
allocation, failed and successful promotion, managed-to-owned copy, owned
drop, root release, and collection. The cycle test verifies that two root
registrations on one handle protect the cycle through one release and that
collection reclaims it only after the final release and guard exit.

## Replayable reserve failures

All fault points are one-based and run in fresh sessions. Object allocation
fails at points 1 (`ObjectPayload`), 2 (`ObjectReferences`), and 3 (`SlotTable`).
Byte allocation fails at points 1 (`ManagedBytes`) and 2 (`SlotTable`). Owned
promotion fails at point 1 (`SlotTable`) while returning the owned value.
`link` fails at point 1 (`Edges`); subsequent collection proves no edge was
written. Owned allocation and managed-to-owned copy each fail at point 1
(`OwnedBytes` and `ManagedToOwned`). Tests assert the primary
`HostAllocation` error, reserve attempt and site, unchanged roots and live
charges, and the expected allocation-failure counter where region accounting
applies. Successful allocations after failure are checked against the model.
The auto-mode oracle checks successful allocation timing at trigger zero;
`no_gc` checks only the guaranteed collection prohibition.

## Verification

| Command | Result |
| --- | --- |
| `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline --quiet` | Passed twice after the final test edit, zero failures. |
| `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-memory-conformance --test faulted_state_model --offline -- --nocapture` | Deterministic replay passed: 4 tests, zero failures. |
| `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --package cgx2-runtime --package cgx2-memory-conformance -- --check` | Passed. |
| `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo clippy -p cgx2-runtime -p cgx2-memory-conformance --all-targets --offline -- -D warnings` | Passed. |

No runtime defect was found by these traces. ASan, Miri, LSan, and concurrent
execution were not run. This result covers the direct runtime only; it does
not establish source/native parity or foreign-runtime safety.
