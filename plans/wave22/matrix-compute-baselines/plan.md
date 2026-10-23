# Larger real CPU matrix compute baselines

**Goal.** Extend Wave 21's valid but tiny float64 CPU measurements to warm shapes with enough work to examine compute cost while preserving fair transfer-inclusive comparisons and uncertainty.

**Ownership.** Edit only `bench/**`. Read but do not edit compiler, LibTorch, binding, notebook, Python bridge, root Cargo files, shared docs, hidden LLM holdout, or scored isolation protocol. This lane cannot depend on concurrent Wave 22 float32 source work.

## Deliverable and contract

1. Keep Wave 21's cold `.cg` source versus C++ question within the source ceiling of 32x32/1,024 elements. Add a separately labeled warm direct-facade versus hand-written LibTorch C++ question at larger bounded shapes, such as 128x128 and 512x512 subject to the adapter's element limit and local time budget. Both routes use the same row-major float64 inputs, CPU thread count, calls, output copy, and handle release. Validate each output against a scalar reference within declared tolerance, then require matching hashes of the same quantized canonical output across routes; retain separate raw output hashes for provenance. Optional Python Torch stays separately labeled and runs only with a matching installed version.
2. Record shapes, repetitions, warmup, timeouts, thread/device settings, and exclusions in bench documentation before new results. Use at least 20 completed warm samples per route for a p95; below that, set p95 to unavailable. Report a bootstrap 95% median interval with at least five completed samples, otherwise mark insufficient samples. Preserve raw per-run wall/CPU/RSS, package/toolchain hashes, failed and stopped runs, and interval methods. Mark cold source validation, adapter/copy, and warm compute phases as measured or unmeasured; do not subtract incomparable whole-process runs.
3. Fail closed for missing/mismatched real pinned Torch 2.11 package, incompatible C++ baseline, wrong output, timeout, or changed closure. Mock data may test parsing but cannot enter scored results. Do not generalize selected CPU matrix shapes to arbitrary `.cg`, GPU, inference models, or LLM authoring.

## Verification and handoff

Run `python3 -m unittest discover -s bench/tests -q`, public syntax-pilot readiness, a local real benchmark when the pinned package exists, and mismatch/timeout harness tests. Commit only `bench/**`; report SHA, rerun command, raw data path, sample counts/intervals, missing routes, and whether provisional runtime targets remain unscored. Stop for review.
