# Wave 24 verified loader cost — measured blocker

This bounded deliverable retains opt-in cost instrumentation and the original
hashing implementation. It does **not** claim a loader speedup. No useful safe
optimization was demonstrated in the buffer-size experiment. Mandatory
full-content checks dominate this small f32 source workload, and caching or
removing those checks would violate the lane contract.

## Reproduction and provenance

Base: `425cb0a5467d98b2cf9565be19067b2f39f23fff`. Linux x86_64, AMD Ryzen
7 9800X3D, Rust/Cargo 1.98.1, default features, release profile, locked offline
dependencies. Exact build settings and binary/toolchain hashes are in
`provenance.json`. The final build's `cgx2` and `cgx2-native` were byte-identical
(`cmp`) to the final measured snapshots. Root Cargo manifest/lock are unchanged.

The private package is at `target/wave24-loader/package`; it contains ordinary
copies of the prepared Torch 2.11.0 CPU f32 facade and dependencies. Copied
closure paths were rewritten to this directory; host libraries remain pinned
at canonical system paths. Package metadata and exact input source are retained
in `inputs/`; all package and closure-content hashes are recorded in each raw
measurement JSON. The copied preparation evidence describes the original
deployment; the copied closure lock and sample hashes describe this deployment.
The package, input hashes and numeric stdout agree across all 20 samples.

From the repository root, the commands were:

```sh
export CARGO_TARGET_DIR="$PWD/target/wave24-loader/build"
export TMPDIR="$PWD/target/wave24-loader/tmp"
cargo +1.98.1 build --locked --offline --release -p cgx2-cli -p cgx2-bind --bins
# Save both cgx2 and its required sibling cgx2-native to base/ before edits,
# and trace/ after instrumentation. Never overwrite another lane's binaries.
python3 crates/cgx2-bind/evidence/wave24/measure.py frozen-base target/wave24-loader/base/cgx2 --prepare
python3 crates/cgx2-bind/evidence/wave24/measure.py before-trace target/wave24-loader/trace/cgx2 --trace
cargo +1.98.1 build --locked --offline --release -p cgx2-bind --example hash_buffers
target/wave24-loader/build/release/examples/hash_buffers target/wave24-loader/package/libtorch_cpu.so
python3 crates/cgx2-bind/evidence/wave24/measure.py final-trace target/wave24-loader/trace/cgx2 --trace
python3 crates/cgx2-bind/evidence/wave24/measure.py final-untraced target/wave24-loader/trace/cgx2
python3 crates/cgx2-bind/evidence/wave24/maps.py target/wave24-loader/base/cgx2 base
python3 crates/cgx2-bind/evidence/wave24/maps.py target/wave24-loader/trace/cgx2 final
python3 crates/cgx2-bind/evidence/wave24/summarize.py
```

Scripts refuse to overwrite sample files. To reproduce, archive existing outputs
and use new labels/directories. Preparation requires the existing reviewed
package `/tmp/cgx2-wave23-boundary-f32`; if absent, recreate one with
`crates/cgx2-libtorch/prepare_pinned.py --matrix-f32` as documented in `PREPARE.md`.
New package locations require new absolute closure paths and hashes.

## Observations

Five independent fresh processes per block, fixed order, no cache eviction,
120 s timeout, OMP/MKL/OpenBLAS threads each set to one. All 20 succeeded;
none timed out. Expected independent result: `[[58,64],[139,154]]`, with
`atol=rtol=1e-5`. Every raw stdout is identical. "Cold" means a new process,
not a cold page cache. No p95 is reported from five observations.

| Release source block | Median seconds | Range seconds |
| --- | ---: | ---: |
| Frozen common base, tracing absent | 6.6616 | 6.6299–6.7035 |
| Instrumented code, tracing enabled | 6.6855 | 6.6387–6.7122 |
| Same instrumented code, later tracing repeat | 6.6445 | 6.6130–6.8058 |
| Final code, tracing disabled | 6.6320 | 6.6281–6.6501 |

These overlapping ranges do not establish a speedup or isolated instrumentation
overhead. No CPU isolation or host-load control was applied. The short owned-test
build overlapped the first later traced repetition; that sample is retained.
All timing blocks follow the preregistered fixed order, so drift is possible.
Raw records are `frozen-base.json`, `before-trace.json`, `final-trace.json`, and
`final-untraced.json`; derived statistics are reproducible with `summarize.py`.

Directly timed phases in the first traced block (per-process totals, medians):

| Scope | Count in every process | Inclusive seconds |
| --- | ---: | ---: |
| Full pinned-content hashing | 429 | 6.5090 |
| ELF dynamic inspection, including readelf subprocesses | 69 | 0.04171 |
| Map reads | 30 | 0.001358 |
| Complete closure preflight | 3 | 0.61405 |
| Package preflight | 2 | 0.41142 |
| Pre-open loaded-module verification | 1 | 0.19782 |
| Loaded/operation-boundary verification | 29 | 5.72493 |

Scopes nest: hashes and ELF work are included in preflight and verification.
Do not add these rows. Both traced blocks have exactly the same counts and
15,010,978,785 hashed bytes per process: 33 passes over the 13 pinned files,
454,878,145 bytes per complete pass. The later block's hash median is 6.4799 s.
The measured hash spans cover about 97% of whole-process time. The loaded-module
census itself includes map parsing, identity checks and ELF inspections; only
map I/O and ELF inspection have separate leaf timers. Native compute, copy,
release, CLI parsing and individual metadata/allocation costs remain unmeasured
separately. No phase is estimated by subtracting incomparable process runs.

The isolated SHA-256 buffer experiment uses the same descriptor, full bytes,
hash implementation and fixed round-robin order, five repetitions per size.
All 15 digests equal pinned `libtorch_cpu.so` SHA-256
`ad0bcc0225bf260968e0464982b78c2f25358400769720ce63c209ffc9929313`.

| Buffer | Median ms | Range ms |
| --- | ---: | ---: |
| 64 KiB | 192.47 | 189.28–212.19 |
| 256 KiB | 191.67 | 189.58–201.06 |
| 1 MiB | 190.52 | 188.99–196.45 |

`hash-buffers.jsonl` retains every observation. This microbenchmark uses heap
buffers consistently across sizes; production retains its original 64 KiB stack
buffer. It does not justify added allocation/stack pressure for a noisy ~1%
median difference. The SHA implementation already dispatches to SHA-NI on this
host; no new backend or dependency was introduced.

The precise measured blocker is 15.01 GB of sequential full-file validation at
the original boundaries, dominated by the 446,534,944-byte Torch CPU library.
Metadata/hash reuse would miss changed same-inode bytes; caching unpinned ELF
SONAMEs by inode could miss same-inode metadata mutation. Removing preflights
or operation checks would remove specified verification boundaries. Optimizing
the ~42 ms of readelf work alone cannot materially resolve the observed ~6.6 s
startup. This is a bounded negative result, not a claim that no future validated
hash implementation can improve throughput.

## Verification and failure accounting

Exact commands/environment and outcomes are retained in `verification.json`
and the corresponding logs:

- `cargo +1.98.1 test --locked --offline -p cgx2-bind -p cgx2-libtorch`:
  **31 passed**, five opt-in real tests initially ignored.
- Release opt-in LibTorch suite, with package/facade/version variables set:
  **all five real tests passed** (f64 vector/matrix, f32 two-cycle reference,
  matrix element/lifetime boundaries, direct real facade).
- `cargo +1.98.1 test --locked --offline -p cgx2-cli --test tensor`:
  **13 passed**; two environment-gated real-source bodies skipped in this
  invocation. The separate release real f32 source test **passed** with the
  required package variable set. The separate f64 source body was not run;
  f64 mock source and real adapter matrix/vector coverage passed.
- Owned Clippy, all targets, `-D warnings`: passed. Owned Rustfmt checks and
  `git diff --check`: passed. Final release binaries match measured snapshots.
- Added a subprocess regression for tracing on/off, exact JSON/status
  preservation on success and hash failure, and non-`1` settings remaining
  silent. Added same-inode, same-size, restored-mtime mutation rejection,
  repeated failure, restoration and legitimate independent-open retry coverage.
  Existing closure substitution, SONAME, malformed/missing metadata, loader
  policy and tensor-lifetime fixtures remain unchanged and passed.

Two initial setup launches failed before a usable measured run. The retained
`setup-missing-helper.json` records the second failure (`cannot start native
helper`), caused by snapshotting only `cgx2` rather than its required sibling
`cgx2-native`. The first failed launch's file was overwritten by that retry
before overwrite protection was added; its exact raw timing is unavailable.
Neither attempt is counted as a successful timing sample. Snapshotting both
binaries fixed setup; no measurement sample was dropped or replaced.

`base-maps.json` and `final-maps.json` retain additional diagnostic polling runs
of the exact hashed launcher and native helper, with Torch CPU mappings observed
and no libpython found. Each binary's `readelf -d` and transitive `ldd` output
also contains no Python. Map polling is every 5 ms and cannot prove absence of
transient mappings between observations. These runs are excluded from latency
samples. No bridge-feature tests were needed or run; no ASan/LSan, GPU, inference
throughput, long-run memory safety or release-gate claim is made.

All original full hashes, inode/path checks, loader-environment policy,
closure membership and conflicting SONAME checks, validation order, failure
categories, before/after-call checks and handle cleanup decisions remain in
place. Instrumentation stores only process-local opt-in configuration, not
trusted library state. A verification failure cannot populate a trust cache.
Filesystem polling does not freeze executable memory: concurrent mutation can
occur between checks, constructors execute during dlopen, and a malicious
facade can violate its ABI. These existing threat boundaries remain unchanged.
