# Wave 23 f32 release observation protocol

Frozen before the local timed run. Common Rust base:
`9b8f6f5610e697429b364528fa32a6b716aa288a`.
Only `bench/**` changes are part of this lane. No Python-isolation changes
are consumed. No release/debug speed comparison is planned.

- Select `--dtype f32 --build-profile release`. Require the copied real CPU
  Torch 2.11.0 v3 package and byte-identical C++ libraries. f64 is a separate
  explicit workload; historical JSON is retained unchanged.
- Cold source and C++: shapes `(2,3,2)` and `(16,16,16)`, 5 independent
  completed process repetitions per route/shape. Source dimensions remain
  at most 32 and each matrix at most 1,024 elements.
- Warm direct C-facade and C++: `(128,128,128)` and `(512,512,512)`, 5 warmup
  calls and 30 completed samples per route. Row-major, CPU, one intra-op and
  inter-op thread; inclusive creation, matmul, output copy and release.
  The f32 facade additionally checks its output shape/dtype via the v3 info
  call; this overhead remains in inclusive timing.
- Fixed `atol=1e-5`, `rtol=1e-5`: round input decimals to binary32 first;
  compute an independent Python scalar reference with binary64 accumulation.
  Require each route to agree with the reference and every route pair to
  agree in both directions. Raw and quantized hashes are provenance, never
  an equality gate. Nonfinite, wrong shape/dtype or tolerance failure
  invalidates the report. Never change tolerances after a failure.
- Per-process timeout 300 s, per-build timeout 600 s, cold route/shape budget
  600 s. Keep failures, timeouts, partial output and stopped runs. No retry
  replaces a failed sample. Publish failed reports alongside a corrected
  rerun if implementation repair is required.
- Median 95% percentile interval: 2,000 bootstrap resamples, seed 20261023,
  sorted indexes 49 and 1949, minimum 5 samples. Nearest-rank p95 requires
  20 completed warm samples. No p99. CPU time from `/usr/bin/time` has 10 ms
  resolution; warm CPU uses the process CPU clock. RSS is the process peak,
  not incremental per-call allocation.
- Build fresh Rust executables offline in a private target directory;
  retain Cargo artifact profiles, executable hashes, build argv/environment,
  C++ Release compile/link commands/cache, compiler versions, package and
  closure hashes. External binaries are `unverified-profile` even if named
  `release`; no optimization claim is inferred from a filename. Record the
  base CLI's actual transitive ELF Python linkage via `ldd`.
- Source validation and closure checks are included in the cold process;
  `cgx2 check` is also measured as a separate whole process. Loader and
  adapter/copy subphases are independently unmeasured. Never subtract
  different whole-process runs. Warm C ABI excludes the Rust pinned loader.
- No working matching Python Torch interpreter is selected locally; record
  this route unavailable. GPU, source parallelism, general inference and
  LLM targets remain unscored.

Exact local run (the linker directory already contains the host's missing
unversioned libpython linker name):

```sh
mkdir -p target/wave23-tmp
TMPDIR="$PWD/target/wave23-tmp" LIBRARY_PATH=/tmp/cgx2-wave22-linker python3 bench/matrix_runtime.py \
  --dtype f32 --build-profile release \
  --package /home/john-riley/projects/lang/target/cgx2-wave21-f32-package \
  --torch-root /home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch \
  --compiler /usr/bin/x86_64-linux-gnu-g++-15 \
  --cold-reps 5 --warm-reps 30 --warmup 5 --timeout 300 \
  --build-timeout 600 --cold-budget-seconds 600 \
  --out bench/results/wave23-f32-release.json
```

Verification:

```sh
python3 -m unittest discover -s bench/tests -v
python3 bench/readiness.py --build --out bench/results/wave23-syntax-readiness.json
```

Readiness is the public syntax pilot only; independent review, scored
isolation and paired model traces may remain unavailable/unrun.

Storage adjustment before any timing: the first attempt used `/tmp` and
failed to copy the pinned library because of the host quota. Its invalid
report is retained as `results/wave23-f32-release-tmp-quota.json`. Use the
workspace disk via `TMPDIR`; numerical and sample settings are unchanged.
A second attempt built verified release Rust binaries but found a C++
template compilation error before timing; its report is retained as
`results/wave23-f32-release-cpp-build-failure.json`. The dependent-template
syntax was repaired before the next run; no timed samples were replaced.

Post-run compatibility verification used the same harness and toolchain with
`--dtype f64 --package /tmp/cgx2-wave19-libtorch-package --cold-reps 1
--warm-reps 1 --warmup 0`, keeping the other run settings unchanged and writing
`results/wave23-f64-compatibility-smoke.json`. This is only a compatibility
smoke; it cannot establish intervals, p95, or precision-mode speed comparisons.

The readiness `--build` recipe targets the original checkout, which is
read-only in this chat. Its failed report is retained. Build the same pinned
compiler locally and run the public checks with:

```sh
LIBRARY_PATH=/tmp/cgx2-wave22-linker cargo build --offline --locked -p cgx2c \
  --target-dir target/wave23-readiness
python3 bench/readiness.py --compiler "$PWD/target/wave23-readiness/debug/cgx2c" \
  --out bench/results/wave23-syntax-readiness.json
```
