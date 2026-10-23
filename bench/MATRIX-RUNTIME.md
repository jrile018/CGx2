# Local real matrix runtime baselines

This harness measures two distinct CPU float64 questions on Torch 2.11.0:
independent cold `cgx2 run PACKAGE` versus a hand-written LibTorch C++
process, and warm repeated direct C facade calls versus the same C++ API.
The optional Python route uses direct `torch.matmul` in a separate process and
is reported separately. The fixed cases are `[2,3] @ [3,2]` and
`[16,16] @ [16,16]`. The source route's current 32x32 literal ceiling limits
what this experiment can say about compute workloads.

From the repository root, with a real schema-2 matrix package produced by
`crates/cgx2-libtorch/prepare_pinned.py --matrix` and a compatible Torch
installation already on disk:

```sh
python3 bench/matrix_runtime.py \
  --package /absolute/path/to/pinned-package \
  --torch-root /absolute/path/to/torch \
  --compiler /absolute/path/to/c++ \
  --out /tmp/cgx2-matrix-runtime.json
```

Add `--python-torch /absolute/path/to/python` only if that interpreter has
CPU Torch 2.11.0. The command builds the CGx2 binaries offline and builds the
C++ baseline from local Torch headers. `--cold-reps`, `--warm-reps`,
`--warmup`, `--timeout`, `--build-timeout`, and `--cold-budget-seconds` adjust
the recorded budgets. The package is cloned into a temporary workspace with
its closure paths rewritten to that clone; original pinned libraries are
hardlinked or copied, never modified. The generated `.cg` source differs only
in the two matrix literals. The report records original package hashes and
built tool hashes. No network access or hidden credentials are used.

The driver validates the package lock/evidence version, byte equivalence of
the C++ baseline's LibTorch libraries, `cgx2-bind check`, C facade identity,
ABI/API version, and every route's output against a scalar reference before
timing. It also compares quantized output hashes across routes. Timed runs
that mismatch, fail, or time out make the whole report `invalid`, with no
scored summary. Cold runs stopped by the budget are recorded as stopped, not
completed. Mock facades cannot pass the real package and library checks.

Raw process records contain wall time, CPU time, peak RSS, argv, stderr, exit
code, and timeout state. Warm records contain each iteration's inclusive
create/copy/matmul/release wall and CPU time, measured matmul segment, and
process RSS. The derived warm non-matmul segment aggregates input creation,
output copy, handle release, and timer overhead; it is not a pure copy cost.
The warm facade route calls the C ABI directly and excludes the Rust pinned
loader's repeated closure checks. The separate `cgx2 check` run measures a whole manifest/source/
closure check; it is not a subtractable phase of `cgx2 run`. Source startup,
validation, and adapter/copy portions cannot be independently instrumented
here and are marked unmeasured. Python import startup is included in Python
cold runs; its warm samples are within an already imported process. Median
95% bootstrap intervals require five samples, otherwise the JSON says
`insufficient_samples`. The provisional 90% throughput and 1.15x p95 targets
in `docs/validation-plan.md` need broader compute-dominant inference cases;
these two small matrix cases do not establish either target or a general CGx2
runtime advantage. The LLM task gate remains unrun.

The Wave 21 local run's [raw JSON](results/wave21-matrix-runtime.json) used
the package at `/tmp/cgx2-wave19-libtorch-package`, Torch headers at
`/home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch`,
`/usr/bin/x86_64-linux-gnu-g++-15`, one cold sample and five warm samples per
route and shape, and a 300 second run limit. This host lacked the unversioned
`libpython3.14.so` linker name, so its offline Rust build used a temporary
`LIBRARY_PATH` symlink to the installed `libpython3.14.so.1.0`. The installed
Python 3.12 Torch files had no working matching Python interpreter on this
host, so the Python route is unavailable in that run. The JSON preserves the
original raw timings; its summary intervals and ratios were recomputed from
those unchanged samples after the run to mark p95 unavailable with five warm
samples.

The exact local rerun command, after the package in the previous paragraph
has been prepared, is:

```sh
mkdir -p /tmp/cgx2-wave21-linker
ln -sf /usr/lib/x86_64-linux-gnu/libpython3.14.so.1.0 /tmp/cgx2-wave21-linker/libpython3.14.so
LIBRARY_PATH=/tmp/cgx2-wave21-linker python3 bench/matrix_runtime.py \
  --package /tmp/cgx2-wave19-libtorch-package \
  --torch-root /home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch \
  --compiler /usr/bin/x86_64-linux-gnu-g++-15 \
  --cold-reps 1 --warm-reps 5 --warmup 2 --timeout 300 \
  --cold-budget-seconds 300 --out /tmp/cgx2-wave21-matrix-runtime-rerun.json
```
