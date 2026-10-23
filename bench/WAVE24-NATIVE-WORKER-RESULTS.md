# Wave 24 native and isolated-worker evidence

The corrected harness passed on frozen common base
`425cb0a5467d98b2cf9565be19067b2f39f23fff`, using Rust 1.98.1 and
CPython 3.14.4 (`/usr/bin/python3` → `/usr/bin/python3.14`, prefix `/usr`,
local unpinned identity). Default-feature CLI/compiler Cargo builds and both
scalar executables were optimized. No concurrent conditional/loader changes were
consumed. This completes the bounded benchmark delivery, not a release gate.

[Preregistration](WAVE24-NATIVE-WORKER-PREREGISTRATION.md) freezes workloads,
controls and accounting. [Harness instructions](NATIVE-WORKER-BASELINES.md) give
reproduction and interpretation details. The raw reports preserve full commands,
source/package/toolchain/binary hashes, fixture and generated source, environment,
affinity/load, wall/CPU/RSS observations, failures, maps and process trees.

Two full series are retained. Run A has an aggregate false validity flag because
its missing-interpreter expectation was CGR019 instead of the actual CGR017 and
ptrace was denied. Every execution pair and recovery passed output/exit checks.
Run B validates the corrected expectation and /proc fallback end to end. No
workload, sample count or target changed between them; neither replaces the other.
The confirmation report's disassembly was losslessly moved to a gzip JSON sidecar
for review size after measurement; its measured harness source/hash remain intact.
The final harness performs that same packaging automatically.

| Route / series | CGx2 median wall (ms) | Baseline median wall (ms) | CGx2 / baseline p95 (ms) | Median paired wall ratio, bootstrap 95% |
| --- | ---: | ---: | ---: | --- |
| Scalar AOT, A | 0.7843 | 0.7398 | 0.8730 / 0.9160 | 1.0504 [1.0184, 1.0945] |
| Scalar AOT, B | 0.9148 | 0.8559 | 1.0973 / 1.2958 | 1.0528 [0.9879, 1.0933] |
| Explicit Python child, A | 19.4237 | 4.1166 | 19.5037 / 4.3350 | 4.7234 [4.6677, 4.7607] |
| Explicit Python child, B | 19.6199 | 4.3645 | 19.9935 / 6.6491 | 4.5134 [4.3900, 4.5736] |

Each row has 30 independent fresh-process pairs, two excluded warmup pairs and
zero excluded measured pairs. The interval estimates the median within-pair
wall ratio, not the ratio of route medians. Independent process pairs on one
host are the sample unit; workloads, inputs and hosts are not independent samples.
Order is the same preregistered deterministic randomized schedule in both series.
Native observations are below a millisecond and include wrapper/process launch;
no claim about compute-only throughput follows. CPU values are quantized at
0.01s (scalar medians zero), wall clock's reported resolution is 1ns, and RSS
resolution is 1KiB. Host CPU, allowed affinity, memory and load are in each report;
no exclusive CPUs, scheduler/frequency pin or idle-host guarantee was applied.

| Peak RSS high-water (KiB) | CGx2 median / maximum | Baseline median / maximum |
| --- | ---: | ---: |
| Scalar AOT, A | 2368 / 2496 | 2360 / 2488 |
| Scalar AOT, B | 2360 / 2512 | 2248 / 2376 |
| Explicit Python child, A | 13822 / 13956 | 8276 / 8356 |
| Explicit Python child, B | 13850 / 13976 | 8290 / 8348 |

Scalar ratio-of-medians runtime is about 1.060x / 1.069x in A/B; maximum-RSS
ratios are 1.003x / 1.057x. These selected observations are within the provisional
1.25x runtime and 1.5x RSS engineering thresholds. They do not establish the
whole-language, allocation-heavy managed, owned-memory or release gates.

The Python baseline computes the same exact isqrt result in the same selected
CPython with `-I -S`. It omits CGx2 parse/check, protocol validation and return-type
checking. The CGx2 route includes startup, copied-scalar transport, return checks
and cleanup. Their total route ratio is not a tiny-call/PyO3 overhead score.
Child-only startup/transport is included in the CGx2 total and otherwise
unmeasured; no unrelated timing is subtracted.

| Separate phase | Run A median ms [min, max] | Run B median ms [min, max] |
| --- | --- | --- |
| Scalar check | 0.8512 [0.8401, 0.9260] | 0.8395 [0.8287, 0.8612] |
| Python declaration check | 0.8186 [0.7587, 0.8721] | 0.7823 [0.7364, 0.8566] |
| Native cgx2c --release build | 104.0906 [103.6995, 105.9693] | 103.8490 [102.0739, 106.4958] |
| Optimized Rust build | 43.7903 [42.3368, 45.4324] | 44.0330 [42.2116, 44.3274] |

Five fresh commands per phase, unique output files, filesystem caches retained;
no phase confidence intervals or p95. Cargo bootstrap builds are setup costs,
retained in raw `setup`. Ordinary scalar `cgx2 run` measured 5.9482ms / 5.9890ms
(one descriptive observation each, no interval). It interprets the source and is
separate from AOT execution. These builds use the frozen native step/error checks;
the Rust comparison has equivalent checked arithmetic but no CGx2 step budget.

The initial 2,000,000-iteration proposal exceeded the frozen 1,000,000-step cap;
`%` and a direct arg_count() call inside a range bound were also unsupported.
All are retained as pre-measurement failures. The final 20,000+arg_count() loop
binds the runtime bound to a local and spells remainder as y-(y/d)*d. y is positive
and at most about 31 billion, safely within i64, so it equals Rust `%`. Both
executables consume runtime arguments and keep the recurrence loop. Full objdump
shows runtime-bound setup and recurrence multiply/divide/subtract in generated
main and a backward recurrence branch in Rust main; the work was not replaced
by a constant. Exact three scalar oracle outputs are retained in raw `oracle`.

Boundary evidence in Run B verifies exact AOT, CLI shim, and native CLI helper
ELF NEEDED/ldd contain no libpython. Native sampled maps are Python-free. Only the
explicit Python route exposes a `/usr/bin/python3.14` child with `-I -S`; its maps
are retained separately. strace attempts failed with PTRACE_TRACEME/PTRACE_SEIZE
Operation not permitted and remain in each audit's `trace_attempt`. The fallback
records sampled executable paths, command lines, process trees and maps. It cannot
prove absence of an arbitrarily short-lived unsampled process. No audit timing
is accepted into execution summaries.

Negative isqrt returned CGR020; wrong return shape returned CGR021; missing
interpreter override returned CGR017. Each had exit 1 and empty stdout, followed
by an exact successful good call. No override silently fell back in process.

Verification: `python3 -m unittest discover -s bench/tests -v` passed 122 tests,
including 10 new harness tests for wrong output/diagnostics, missing executables,
nonzero/timeout with process-group cleanup, provenance mismatch, sample gates,
randomized pair accounting, retained failures, lossless archives and overwrite
protection. Public syntax readiness passed the unchanged toolchain pins and all
six fixture pairs using the supplied pinned compiler and a private TMPDIR. Its
first default-TMPDIR report retained two build failures with no underlying
compiler stderr exposed by the existing readiness tool; the reason is unconfirmed.
Candidate cgroup isolation remains unsupported; genuine paired model/human
traces, GPU/inference/LLM claims, and the old in-process PyO3 target remain unrun.

Committed evidence:

- `results/wave24-setup-failures.json`: setup syntax/step-limit failures and preparatory release build log.
- `results/wave24-range-check-failure.json`: failed harness check before accepted execution pairs.
- `results/wave24-native-worker-baselines.json`: complete first series A, including failed audit/expectation gates.
- `results/wave24-native-worker-confirmation.json`: corrected successful series B.
- `results/wave24-native-worker-confirmation.json.assembly.json.gz`: lossless full disassembly.
- `results/wave24-benchmark-tests.log`: full final suite.
- `results/wave24-syntax-readiness*.json` and `.log`: initial failure and private-temp success.

Exact collection commands (from the attached worktree):

```sh
python3 bench/native_worker_baselines.py --out bench/results/wave24-native-worker-baselines.json
python3 bench/native_worker_baselines.py --out bench/results/wave24-native-worker-confirmation.json
python3 -m unittest discover -s bench/tests -v > bench/results/wave24-benchmark-tests.log 2>&1
python3 bench/readiness.py --compiler bench/target/wave24-local/cargo/release/cgx2c --out bench/results/wave24-syntax-readiness.json
TMPDIR="$PWD/bench/target/wave24-local/tmp" python3 bench/readiness.py --compiler bench/target/wave24-local/cargo/release/cgx2c --out bench/results/wave24-syntax-readiness-private-tmp.json
```

Historical output names above already exist: choose new names for reruns. The
preparatory compiler was built with `TMPDIR=$PWD/bench/target/wave24-local/tmp
CARGO_TARGET_DIR=$PWD/bench/target/wave24-local/cargo cargo +1.98.1 build --offline
--locked --release -p cgx2c -p cgx2-cli`. Its exact hash is in readiness evidence;
Run A/B independently built the same frozen source in their unique directories.
No legacy original-checkout build was attempted or overwritten.
