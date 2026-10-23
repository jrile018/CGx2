# Wave 20 source/native memory evidence and release gate

This is bounded conformance evidence, not a proof of general memory safety. The
source corpus is `crates/cgx2c/tests/memory_release_*.cg`; the executable
comparison is `crates/cgx2c/tests/memory_release_parity.rs`. Each seed below
is an exact replay label, with no random draws. All byte counts include CGx2
runtime metadata. The region census lists the explicit manual region after
execution; the default auto region remains open with zero committed/live bytes,
zero failures, and zero collections in every row.

| Seed and source | Exact operation sequence | Expected exit/output/first failure | Expected manual region and roots |
| --- | --- | --- | --- |
| `0x20a001`, `memory_release_normal.cg` | create manual 1024; allocate parent object (8 bytes, one reference) and child object (4 bytes, zero references); link; drop child variable; collect; clear link; drop parent; collect; close; return normally from `finish()` and `main()` | 0, empty stdout/stderr, no diagnostic | closed, committed/live 0, peak 180, failures 0, collections 2, collected 180; no roots after close |
| `0x20a002`, `memory_release_nested.cg` | create manual 512; enter outer `no_gc(256)`; allocate parent; enter inner `no_gc(128)`; allocate child; link and drop child variable; exit inner; clear link and drop parent; exit outer; close; return normally | 0, empty stdout/stderr, no diagnostic | closed, committed/live 0, peak 180, failures 0, collections 0; no roots after close |
| `0x20a003`, `memory_release_budget_failure.cg` | create manual 512; enter `no_gc(512)`, then `no_gc(128)`; allocate held 8-byte value; attempt 32-byte allocation; unwind before both explicit drops and close | 1, empty stdout, `CGR013` / `BudgetExceeded` at source span `226..270` | open, committed/live/peak 72, failures 1, collections 0; failure snapshot 1 rooted handle / 1 registration, final unwind 0 / 0, cleanup errors empty |
| `0x20a004`, `memory_release_reserve_failure.cg`, reserve index 4 | create manual 512; allocate held 8-byte value; inject one host reserve failure while allocating the object (8 bytes, one reference); unwind before drops and close | 1, empty stdout, `CGR013` / `HostAllocation` at `158..205` | open, committed/live/peak 72, failures 1, collections 0; failure snapshot 1 / 1, final 0 / 0, cleanup errors empty |
| `0x20a005`, same source, reserve index 5 | same sequence, second fallible reserve site in object allocation | same `CGR013` / `HostAllocation` and span | same census as index 4 |
| `0x20a006`, same source without fault | create manual 512; allocate held and object; drop both; close | 0, empty stdout/stderr | closed, committed/live 0, peak 184, failures 0, collections 0 |

The test compares interpreter and native debug/release exit class, output,
first diagnostic and span, aggregate memory counters, rollback preservation,
and the runtime root census at the failure and final unwind snapshots. A
native nonzero exit is a failure case, never counted as a success parity pass.
`cgx2 run` has no fault injection flag, so seeds `0x20a004` and `0x20a005`
use the explicit test-only interpreter and `cgx2c --test-fail-reserve` paths;
their unfaulted source is `0x20a006`. `cgx2 run` gave empty output and exit 0
for seeds `0x20a001`, `0x20a002`, and `0x20a006`, and `CGR013` at `226..270`
with exit 1 for `0x20a003`.

## Direct-runtime-only boundary

The source memory profile does not expose duplicate root registration,
explicit early `return`, or a catchable host panic. Its memory statement
parser accepts an operation subset and reports unsupported syntax for these.
The existing direct-runtime corpus remains separate:
`crates/cgx2-runtime/tests/unwind_replay.rs` with exact seeds
`0x19a001` (early, object payload reserve), `0x19a002` (panic, object payload),
`0x19a003` (early, object references reserve), and `0x19a004` (panic, object
references reserve), defined in `corpus/wave19-unwind-traces.csv`. It checks
nested guard unwinding, reserve rollback, duplicate root release, region close,
idempotent cleanup, and preservation of the first injected error. These are
host API cases, not source/native parity claims. Successful source execution
exposes a memory report but no root census, so the success rows' final zero
roots are inferred from legal close and checked directly only in the runtime
corpus. Failure snapshots do expose and compare actual runtime root census.

## Dynamic instrumentation

On this host, `rustc 1.98.1`/`cargo 1.98.1` built the regular native corpus.
Nightly `rustc 1.101.0-nightly (5c543b0b8 2026-09-29)` built the generated
source and the embedded `cgx2-runtime` with AddressSanitizer. The temporary
`/tmp/cgx2-wave20-asan-rustc` wrapper contained
`exec rustc +nightly -Zsanitizer=address "$@"`. The generated process attempts
were:

```sh
target/debug/cgx2c build crates/cgx2c/tests/memory_release_normal.cg --out /tmp/cgx2-wave20-normal-asan --rustc /tmp/cgx2-wave20-asan-rustc
ASAN_OPTIONS=detect_leaks=0 CGX2_MEMORY_REPORT=/tmp/cgx2-wave20-normal-asan.json /tmp/cgx2-wave20-normal-asan
target/debug/cgx2c build crates/cgx2c/tests/memory_release_nested.cg --out /tmp/cgx2-wave20-nested-asan --rustc /tmp/cgx2-wave20-asan-rustc
ASAN_OPTIONS=detect_leaks=0 CGX2_MEMORY_REPORT=/tmp/cgx2-wave20-nested-asan.json /tmp/cgx2-wave20-nested-asan
target/debug/cgx2c build crates/cgx2c/tests/memory_release_budget_failure.cg --out /tmp/cgx2-wave20-budget-asan --rustc /tmp/cgx2-wave20-asan-rustc
ASAN_OPTIONS=detect_leaks=0 CGX2_MEMORY_REPORT=/tmp/cgx2-wave20-budget-asan.json /tmp/cgx2-wave20-budget-asan
target/debug/cgx2c build crates/cgx2c/tests/memory_release_reserve_failure.cg --out /tmp/cgx2-wave20-reserve-asan --test-fail-reserve 4 --rustc /tmp/cgx2-wave20-asan-rustc
ASAN_OPTIONS=detect_leaks=0 CGX2_MEMORY_REPORT=/tmp/cgx2-wave20-reserve-asan.json /tmp/cgx2-wave20-reserve-asan
```

The normal and nested builds exited 0 with no sanitizer diagnostic. The budget
failure and reserve-index-4 builds exited 1 with their
expected `CGR013` first diagnostic and no sanitizer diagnostic. The wrapper
instruments both staged runtime and generated executable through the `--rustc`
boundary. Leak detection was disabled because the host's `ptrace_scope` blocked
LeakSanitizer startup in the Wave 19 trial; this run makes no leak claim.
ASan observes only these generated process paths, not interpreter execution,
foreign Python/LibTorch memory, parallel workers, or every legal source program.

## Release-gate checklist

| Claim | Direct runtime | Source/interpreter | Native debug/release | Foreign | Parallel | Gate |
| --- | --- | --- | --- | --- | --- | --- |
| `no_gc` performs zero CGx2 collections on nested success/failure | Wave 19 guard corpus | Seeds `0x20a002`/`0x20a003`, 0 collections | Same two rows, 0 collections in both profiles | No evidence for foreign collectors | No shared-session worker path tested | Open for full supported semantics |
| Failed allocation is bounded, rolls back only the attempted allocation, preserves first error | Wave 19 reserve corpus | Budget and fault snapshots, `CGR013`, no cleanup errors | Same snapshots and first error in both profiles | No faulted foreign allocator evidence | No worker fault path | Open |
| Roots and region close do not yield invalid access, double free, or escape | Wave 19 duplicate-root/close corpus | Success close and failure unwind; success root census unavailable | Success close and failure root census; ASan on generated cases | Foreign handle lifetime remains separate | Parallel ownership remains separate | Open |
| Manual region collects only when requested and region isolation holds | Prior direct-runtime state models | Normal 2 explicit collections; nested and failure 0 | Same counters in both profiles | Foreign GC outside CGx2 counters | No parallel shared region | Open |
| General memory safety of first-release CGx2 | Finite runtime corpus | Finite source corpus | Finite native corpus and scoped ASan | Incomplete | Incomplete | **Open: no general proof or release claim** |

All tests are bounded for ordinary CI. No stress test is added to the default
suite; the existing larger state-machine and fuzz trials remain separate.
