# CGx2 measured CPU policy

This crate runs two reviewed pure CPU primitives: checked i64 elementwise
`value * multiplier + addend` (`execute_map`) and checked two-input
`left[i] * right[i] + addend` (`execute_zip_map`). Sequential and scoped-thread
paths return identical values, output order, and lowest overflow index. Zip map
rejects unequal lengths and infeasible explicit worker counts before dispatch.
`Policy::Auto` is the default and uses
sequential CPU without a validated local profile. Explicit `ParallelCpu` still
requires at least two workers and 2,048 elements. Explicit `Gpu` is unsupported.

## Record, validate, and use local measurements

Calibration is opt-in. It never runs from either execution call. Use a release build and
save both raw samples and a profile on the machine that will execute the map:

```sh
RUSTUP_TOOLCHAIN=1.98.1 cargo run -p cgx2-execution-policy --release --offline --bin calibrate -- \
  --sizes 8192,65536,1048576 --workers 2,4,8 --samples 7 --holdout-samples 5 --warmups 2 \
  > samples.csv
RUSTUP_TOOLCHAIN=1.98.1 cargo run -p cgx2-execution-policy --release --offline --bin profile -- \
  build --raw samples.csv --out profile-1048576-4.txt --workers 4 --size 1048576 --ttl-hours 24
RUSTUP_TOOLCHAIN=1.98.1 cargo run -p cgx2-execution-policy --release --offline --bin profile -- \
  run --raw samples.csv --profile profile-1048576-4.txt --workers 4 --sizes 1048576,65536
```

`profile run` prints backend, reason, cost status, and a deterministic result
hash for each size. Omit `--raw` and `--profile` to see the uncalibrated
sequential fallback. A missing or rejected saved profile also leaves Auto on
that fallback. A profile covers its recorded element range; `build --size`
creates a precise single-size profile from a multi-size CSV. Sizes outside the
range fall back. Callers can use `profile::load_profile` and put its returned
`Measurements` into `Capabilities`; on validation error, leave `measurements`
as `None`.

CSV rows are versioned and contain separate calibration and held-out repetitions
of full `execute_map` latency, including planning, output allocation, worker
launch, and join. The profile records schema and workload IDs, OS, architecture,
CPU model, available threads, Rust version, worker count, size range, sample
counts, creation and expiry times, raw CSV checksum, end-to-end cost intervals,
and maximum held-out regret for Auto's selected backend. The loader checks
those fields against the raw CSV and local machine before producing measurements.
Each backend needs at least three paired repetitions in each phase at every
included size; samples with more than a 2× within-phase spread are rejected.
The interval spans every
observed timing and adds 10% to the upper bound. Auto selects parallel only if
that upper bound is strictly below the sequential lower bound. Profiles expire
after at most seven days; the CLI defaults to 24 hours. No estimate is made
from one run.

The raw hash is a non-cryptographic integrity checksum. Keep raw data and
profiles locally and treat them as untrusted if obtained from elsewhere. A
profile cannot prove future performance under different contention or thermal
conditions. [The Wave 8 local measurement](measurements/2026-10-22-report.md)
records this host's sample path, selections, regressions, and uncertainty.

## Two-input zip map calibration

This example checks sequential/parallel equality, warms both paths, records seven
calibration and five held-out paired samples (alternating timing order), and saves
local raw CSV plus a 24-hour profile if it passes validation. Timings cover the
complete `execute_zip_map` call: planning, allocation, launch, and join.

```sh
RUSTUP_TOOLCHAIN=1.98.1 cargo run -p cgx2-execution-policy --release --offline --bin calibrate_zip -- \
  1048576 2 /tmp/zip-samples.csv /tmp/zip-profile.txt
```

Zip CSV uses schema 2 and `checked-i64-zip-map-v1`; the original scalar map
retains schema 1 and `checked-i64-map-v1`. Use `profile::load_zip_profile`
with the saved profile, the **same raw CSV**, `current_machine()`, worker count,
and current time; attach its `Measurements` to `Capabilities`. If loading fails,
leave `measurements: None`. A scalar map profile cannot enable zip Auto, or vice
versa. Both profiles use all paired calibration and holdout samples, reject
missing or noisy samples, and require the parallel upper latency bound (including
a 10% margin) to be below the sequential lower bound. The checksum detects
accidental edits, not a malicious profile author. Keep profiles local and treat
the measured decision as workload and host specific.

## Scope

`Auto` can choose parallel only for the two reviewed pure checked CPU primitives. It
does not parallelize arbitrary `.cg` source or share managed `Session` handles.
Effects, aliases, `no_gc`, memory activity, reductions, and floating-point work
are excluded. There is no executable GPU backend; even a favorable mock cost
cannot make GPU available. Future source integration requires checked IR and
effect, ownership, and memory guarantees before lowering `.cg` code into either
primitive. GPU execution additionally requires
real operation/device support and measured transfer and synchronization costs.
