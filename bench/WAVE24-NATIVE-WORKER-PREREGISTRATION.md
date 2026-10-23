# Wave 24 frozen native and isolated Python baselines

Frozen before any workload execution on 2026-10-23. Compiler/runtime source base:
`425cb0a5467d98b2cf9565be19067b2f39f23fff`. Only bench paths may change.
Rust 1.98.1; offline locked default-feature Cargo release build; cgx2c --release;
Rust baseline --edition=2024 -C opt-level=3. No concurrent lane commits consumed.
Each invocation creates a private bench/target/wave24-* directory, including TMPDIR.
No external binary profile is trusted without matching source, flags and hashes.

Scalar case: 20,000+arg_count() iterations of x=(31*x+i)%1000000007, seed
arg_count()+17; paired inputs repeat [], [a], [a,b] ten times. Both executables
read runtime arguments (including in Rust), preventing replacement by a constant.
All arithmetic stays within i64; Rust explicitly checks multiplication/addition.
The independent Python oracle is outside timing. This is a scalar arithmetic and
range-loop workload, with no allocation-heavy or owned-memory release inference.

Python case: one allowlisted copied i64 math.isqrt(9223372030926249001), exact
result 3037000499. cgx2 run includes parse/check, worker startup, protocol,
validation and teardown. The same selected local, unpinned CPython executable
runs the Python fixture with -I -S in a fresh process. That baseline omits CGx2
parse/check and protocol validation. No subtraction estimates worker overhead.
Ordinary scalar cgx2 run is separately labeled interpreted execution, not AOT.

Each execution case: 2 fresh-process warmup pairs, then 30 fresh-process pairs,
within-pair order from random.Random(240024), precomputed before execution.
No retries replace observations. Five independent check and native-build pairs
are reported separately; repeated compiler processes write unique output paths.
One ordinary scalar interpreter observation is descriptive, without intervals.
Deadlines: execution/check 30 seconds, native builds 120 seconds, Cargo 600 seconds.
Threads: OMP/MKL/OPENBLAS/NUMEXPR=1, RAYON_NUM_THREADS=1, LC_ALL=C,
PYTHONHASHSEED=0 (ignored by isolated CPython); record affinity/load and inherited
build environment. CPU affinity is held at the parent's allowed mask, no exclusive
CPU or frequency pin. No site packages, GPU, torch, inference, LLM or human trials.

Accept timings only on exact stdout, expected exit and diagnostics; keep all
failures/timeouts and their commands. Warmups, boundary audits, diagnostics and
build setup are excluded from execution summaries and retained. Wall time is
perf_counter around process startup/wait (including GNU time wrapper); user/system
CPU and peak RSS from GNU time include waited descendants. RSS is a process-tree
high-water observation, not live memory or a sum of concurrent resident pages.
GNU time CPU resolution 0.01s; RSS resolution 1 KiB; wall clock resolution recorded.

Report median, p95 only with >=20 completed samples, and deterministic 95% paired
bootstrap median-ratio interval (5,000 resamples, seed 240025) with >=30 valid
pairs; five build/check runs have medians/ranges only. Sample unit is one fresh
process pair on this host, not an independent machine/workload. Retain incomplete
pairs, exclude them from ratios with explicit counts. No p99. Native provisional
1.25x runtime / 1.5x RSS comparisons apply only to these observations; no overall
release gate. Old retained in-process PyO3 tiny-call target is unscored and separate.

Audit exact executable ELF NEEDED/ldd and sampled maps; no libpython in AOT or
native CLI. Untimed strace exec evidence must show worker only on explicit Python
route. Probe negative isqrt (CGR020), wrong return shape (CGR021) and missing
interpreter (CGR019), requiring empty stdout and a subsequent successful run.
An explicit missing interpreter override must fail without in-process fallback.

Run all bench tests and public syntax readiness with unchanged toolchain lock.
Use an explicit locally built --compiler if the legacy build recipe is unwritable;
retain any actual failed attempts. Evidence will record command and profile hashes,
all toolchain/package/fixture hashes, ELF/maps/exec observations, exclusions and
local interpreter executable/version/prefix. No private answers are read or changed.

Pre-measurement amendment: initial syntax smoke revealed missing build subcommand,
missing foreign declaration `=`, unsupported `%`, and the 1,000,000 execution-step
cap. Full repeated failure observations are in results/wave24-setup-failures.json.
The CG fixture spells remainder as y-(y/1000000007)*1000000007; all y are positive,
so this exactly matches Rust %. The amended loop bound depends on runtime argument
count and fits the frozen step budget. No accepted measurements preceded amendment.

Second pre-measurement syntax amendment: calling arg_count() directly in a range
bound is rejected (CGT012). Retained the full failed harness attempt in
results/wave24-range-check-failure.json. Bind the runtime bound to local `iterations`
before the loop; it passes the frozen compiler. No execution pairs had run.

Post-run instrumentation correction (all first-run timings retained, no target
changed): the frozen CLI uses CGR017 for missing interpreter, not the initially
assumed CGR019. Run A completed every pair and all expected failures/recoveries,
but its aggregate validity flag remains false from that expectation and blocked
ptrace. The corrected harness uses CGR017 and falls back to untimed /proc process
and command-line/maps sampling when strace is blocked; it retains trace failures.
Sampling cannot exclude arbitrarily short-lived unobserved processes. Run B repeats
the same schedule solely to validate the corrected harness end to end; report both
series, including their timings, with no selection between them. All timed workload
sources, release settings, inputs, controls and sample counts remain unchanged.
