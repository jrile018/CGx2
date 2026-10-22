# Wave 14 memory release audit

The first-release memory correctness gate remains **open**. These tests exercise
the frozen single-threaded `Session` graph API; a finite passing corpus is
evidence for the cases below, not a universal memory-safety proof.

| Requirement | Wave 14 evidence | Gate status |
| --- | --- | --- |
| Interpreter graph semantics | `release_graph_audit.rs` compares logical node reachability, checked slots, root counts, stale handles, close, nested `no_gc`, and per-region plus total reports after each transition. The direct runtime model does not establish source checker or interpreter parity. | Open pending source-level end-to-end graph fixtures. |
| Native graph parity | The native source graph lane owns compiler and native parity work. This audit uses only the frozen runtime `Session` API. | Open pending native/interpreter graph output and error comparison. |
| Foreign tensor lifetime | LibTorch owns tensor/device allocations. They are excluded from CGx2 region charges and this object graph oracle. | Open pending direct tensor handle, alias, release, and foreign allocator audit. |
| Thread safety | `Session` is single-threaded; no shared-session concurrency guarantee or test is present. | Open for any future shared-session claim. |
| Sanitizers and dynamic tools | No sanitizer or Miri run is recorded for this Wave 14 lane. Ordinary Rust tests cannot detect every invalid access. | Open pending tool runs and safety review. |

## Reproducible bounds

`tests/release_graph_audit.rs` runs seeds `0..64` with the wrapping LCG
`state = state * 6364136223846793005 + 1`, upper 32 bits modulo each choice.
Each trace has 15 fixed operations, 80 generated operations, and 6 closing
operations (at most 101); there are at most 128 logical nodes, two named
regions (manual and auto), two checked slots per object, one-byte payloads,
and a 30-second test timeout. The auto trigger is `usize::MAX`, so the model
tests explicit collection without an implicit allocation safepoint. The fixed
prefix proves a rooted parent retains a child and cycle, then clearing the
last parent edge allows the unreachable cycle to be collected. It also forces
a close-with-roots rejection and nested `no_gc` collection and allocation
rejections. Every operation compares exact result class, live/stale handles,
stored slots, region counters, and total report counters. On a mismatch the
test prints the seed, failing step, and a deletion-minimized operation trace.
Pause durations are deliberately excluded because they are wall-clock data.

`tests/release_faults.rs` checks object payload/reference/slot reservation
ordering, legacy edge growth, manual-limit precedence, fixed-point collection,
rollback, later successful use, stale handles, and release exactly once. The
existing `tests/fault_injection.rs` also covers region-table reservation,
managed bytes, owned bytes, promotion, and managed-to-owned copy. The current
collector scans slots to a fixed point and has **no host worklist reservation**:
an armed reserve fault survives collection and fires at the next actual
reservation. Checked `set_reference` updates preallocated slots without a
host reservation; `link` is the edge operation with a fallible reserve.

Run both owned crates offline with:

```sh
RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib \
  cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline
```

`/tmp/cgx2-python-lib` is the local linker path for the Python dependency of
the conformance CLI. It does not make foreign allocation part of this audit.
