# Wave 23 release f32 observations

[Raw f32 report](results/wave23-f32-release.json) and
[preregistered settings/rerun command](WAVE23-F32-PREREGISTRATION.md).
The measured Rust base is `9b8f6f5610e697429b364528fa32a6b716aa288a`;
only benchmark changes were present. The report is valid with no failed,
timed-out, or budget-stopped samples. No settings were adjusted after timing.

The CLI, native helper and binder were freshly built offline. Cargo artifact
profiles report `opt_level=3`, `debug_assertions=false`, `debuginfo=0`.
The C++ worker used GCC 15, CMake Release `-O3 -DNDEBUG`, C++20. Exact compile
and link commands, Cargo artifact records, executable hashes, tool versions,
source hashes, package/closure hashes, thread/device/shape settings, raw
wall/CPU/peak-RSS samples and exclusions are in the report. The build used
the existing local libpython linker-name workaround; it does not remove
Python linkage. `ldd` on the exact CLI reports libpython; the native helper
has no libpython ELF dependency. This is an ELF linkage observation, not an
independent `/proc` mapped-library census.

All source, C++ and direct C-facade outputs passed the independent scalar
reference and the route-pair checks with fixed `atol=rtol=1e-5` after inputs
were rounded to binary32. Maximum observed absolute error was
`5.010515451431274e-7`. Raw/quantized output hashes remain provenance and are
not required to match. f32 uses the v3 typed create, matmul, info and copy
functions; these are actual float32 tensors from the pinned real CPU Torch
2.11.0 package.

Cold values include the whole process and full pinned source execution.
Each route/shape completed five independent processes; intervals are the
preregistered bootstrap 95% median intervals. Source validation is also
measured by a separate `cgx2 check`; its time is not subtracted from run time.
Loader and adapter/copy costs are not independently measured.

| Cold shape M,K,N | Source median seconds (95% interval) | C++ median seconds (95% interval) |
| --- | ---: | ---: |
| 2,3,2 | 6.4131 (6.3851–6.4342) | 0.10351 (0.09781–0.10703) |
| 16,16,16 | 6.4064 (6.3887–6.4338) | 0.10312 (0.09869–0.14007) |

Warm values include creation/copy, matmul, output copy and release within an
already initialized process. Each route/shape has 30 completed samples after
five warmups. Facade timing includes the v3 output shape/dtype check and
excludes the full Rust pinned-loader closure checks. C++ and facade share
CPU, layout, binary32 inputs, one intra-op/inter-op thread and pinned
libraries. RSS is a process high-water mark; CPU resolution differs between
whole-process `/usr/bin/time` and the warm process CPU clock.

| Warm square size | C++ median ms (95% interval) | Facade median ms (95% interval) | C++ / facade p95 ms |
| --- | ---: | ---: | ---: |
| 128 | 0.033996 (0.033601–0.034216) | 0.047475 (0.046991–0.048121) | 0.035670 / 0.059091 |
| 512 | 2.306069 (2.303749–2.308259) | 2.538426 (2.536742–2.540212) | 2.320309 / 2.547662 |

The nearest-rank warm p95 has the required minimum 20 samples. No cold p95
or p99 is claimed. These are selected observations; the facade is slower
than C++ in these runs. No debug comparison was run, so there is no
release/debug startup speed claim. General inference, GPU, source
parallelism, LLM targets and a Python-free CLI gate remain unscored. Optional
Python Torch is unavailable because no working matching interpreter was
selected; it is never replaced by a different precision or installation.

Verification and retained limitations:

- [All 112 benchmark tests pass](results/wave23-benchmark-tests.log), including
  wrong dtype/layout/shape, version mismatch, timeout/exclusion preservation,
  fixed tolerance and route-pair disagreement, sample gates, f32 input
  rounding/source caps, and external profile provenance.
- [Real f64 compatibility smoke](results/wave23-f64-compatibility-smoke.json)
  is valid for all four existing shapes. One cold/warm sample per route,
  zero warmups: insufficient for median intervals or p95 and no speed claim.
  Historical Wave 21/22 files were left unchanged.
- [Public syntax readiness](results/wave23-syntax-readiness.json) passes
  toolchain pins and task equivalence using a fresh local `cgx2c`. Cgroup
  CPU/memory/PID delegation is unavailable; independent review and paired
  model traces remain unrun. The frozen `--build` recipe points at the
  read-only original checkout and failed; that
  [attempt](results/wave23-syntax-readiness-build-failure.json) is retained.
  The same source was then built offline in this worktree with
  `cargo build --offline --locked -p cgx2c --target-dir target/wave23-readiness`
  and supplied to readiness with `--compiler`.
- Before any timed samples, `/tmp` copying hit its quota and the next
  attempt caught a C++ dependent-template compile error. Both failed setup
  reports are retained:
  [storage](results/wave23-f32-release-tmp-quota.json),
  [compile](results/wave23-f32-release-cpp-build-failure.json).
  Workspace temporary storage and repaired syntax were used for the valid
  run; no timed sample was discarded or replaced.

The benchmark source hashes in both valid matrix reports match the committed
worker/driver/CMake files. `git diff --check` passes. No root Cargo, shared
design/ledger, compiler or other lane files were changed.
