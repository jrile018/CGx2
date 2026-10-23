# Native and isolated Python process baselines

`native_worker_baselines.py` is a bounded Linux-only, standard-library harness for
the public `fixtures/native-worker/v1` cases. See
[preregistration](WAVE24-NATIVE-WORKER-PREREGISTRATION.md) and
[Wave 24 results](WAVE24-NATIVE-WORKER-RESULTS.md). It requires GNU time, readelf,
ldd, objdump, strace (with /proc fallback), Rust 1.98.1 and CPython. It uses only
the committed common compiler base; it rejects any tracked non-bench source drift.
No private task inputs are part of these workloads.

From the attached checkout, run:

```sh
python3 bench/native_worker_baselines.py --python /usr/bin/python3 \
  --out bench/results/wave24-local-new.json
```

Use a new output name each time. Existing evidence cannot be overwritten.
Each run builds default-feature `cgx2c` and `cgx2-cli` with
`cargo +1.98.1 build --offline --locked --release -p cgx2c -p cgx2-cli`,
inside a new `bench/target/wave24-*` target and TMPDIR. No other lane's artifacts
are used. Binaries are retained there; raw reports include their hashes,
source revision and all tracked non-bench hashes, Cargo.lock, build command,
toolchain executables/hashes/versions, fixture bytes/hashes, generated Rust,
ELF dependencies and runtime audit maps. Full disassembly is preserved as a
lossless deterministic gzip JSON sidecar. Reports also embed the measured harness
source/hash so later harness changes do not erase the actual measurement recipe.
The work directories are ignored by Git, not automatically removed.

To supply existing binaries, use `--binary-dir DIR`. They are labeled
`unverified-profile` by default. `--build-provenance FILE` accepts the local
build-provenance.json created by this harness only when recorded base, toolchain,
Cargo.lock, exact release/default-feature command and each binary's hash match.
It verifies a local build record, not a signed supply-chain attestation. Preserve
that record and original setup observation when moving binaries. A failed override
never falls back to another binary/interpreter or to in-process Python.

The scalar AOT case measures an already-built executable against optimized Rust.
Checking, native compilation, and ordinary scalar `cgx2 run` are distinct phases.
Python `cgx2 run` includes CGx2 validation and an isolated child; the CPython
baseline omits CGx2 parsing/checking and protocol handling. Both include fresh
process startup/teardown. Child-only startup/transport costs are unmeasured; do
not subtract the two routes to invent such a cost. Retained in-process PyO3
benchmarks and their tiny-call target remain separate and unscored here.

Wall observations include the GNU time wrapper and process launch. CPU has 10ms
resolution; peak RSS has 1KiB resolution and is a high-water observation across
the process and waited descendants, not a simultaneous resident-byte sum. Small
native workloads are particularly sensitive to launch noise. Every failed or
incomplete pair remains in raw evidence and is counted; p95 needs 20 successes,
and paired bootstrap intervals need 30 complete valid pairs. Phase medians/ranges
use five fresh command invocations with no interval. Warmups are separate fresh
processes, not a persistent interpreter/JIT warm state. /proc audits are untimed;
sampling cannot exclude arbitrarily short-lived unobserved processes.

Verification and public readiness:

```sh
python3 -m unittest discover -s bench/tests -v
mkdir -p bench/target/readiness-tmp
TMPDIR="$PWD/bench/target/readiness-tmp" python3 bench/readiness.py \
  --compiler bench/target/wave24-REPLACE/cargo/release/cgx2c \
  --out bench/results/wave24-readiness-new.json
```

Use the actual run's `work_directory` in place of `wave24-REPLACE`. The public
pilot's toolchain lock is unchanged. A matching supplied compiler avoids the
legacy recipe's original-checkout target path. Readiness can still report
unsupported candidate isolation and absent genuine model/human traces; those are
separate unrun gates.
