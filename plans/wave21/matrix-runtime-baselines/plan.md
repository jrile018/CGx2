# Real matrix runtime baselines

**Goal.** Test whether CGx2's direct native matrix route has a measurable runtime purpose, with behaviorally equivalent baselines and honest uncertainty.

**Ownership.** Edit only `bench/**`. Read but do not edit compiler, LibTorch, notebook, execution-policy, root Cargo files, or shared docs. Do not open the hidden LLM holdout or change its scoring protocol.

## Deliverable

1. Add a reproducible local benchmark driver for two separate questions: cold end-to-end `.cg` source package invocation, and warm repeated direct facade multiplication. Use pinned Torch 2.11 CPU float64 package and compare each question with a corresponding hand-written LibTorch C++ baseline. Separately report source startup/manifest and closure validation, adapter/copy costs, and warm multiplication where instrumentation permits; label any unavailable phase as unmeasured. Match thread and environment settings and equivalent input/output transfers for a fair warm comparison. Add direct Python `torch.matmul` only when a compatible pinned Python Torch is actually installed; report separately and include Python startup. Match shape, dtype, row-major input, thread count, device, numeric tolerance, warmup, synchronization, and output hash before timing. Never compare different work under a claimed speedup.
2. Include a tiny 2x3 by 3x2 correctness case and at least one larger compute case within each supported route's size limit. Report cold startup separately from warm calls; include closure validation and adapter overhead in the end-to-end measure. Record raw per-run wall time, CPU time where available, peak RSS, repetitions, exclusions, toolchain hashes, package hashes, host/core count, and confidence interval or an explicit insufficient-sample status. Timebox expensive cold repetitions and record stopped runs rather than fabricating completed samples. A 32x32 source-route ceiling limits conclusions to tiny shapes; do not infer general compute speedup.
3. Make the harness fail closed when the real pinned package or baseline is missing, output differs, a run times out, or package versions differ. Mock facade data may test harness parsing but may not enter scored results. Include a command that another worker can run locally without network access or hidden credentials, and a small harness test for mismatch/timeouts.
4. Compare observations with the provisional targets in `docs/validation-plan.md` without changing them after viewing results. Do not generalize one CPU matrix to arbitrary `.cg`, PyTorch models, GPU, or LLM authoring speed. The LLM task gate remains unrun without genuine paired model traces and independent task review.

## Verification and handoff

Run `python3 -m unittest discover -s bench/tests -q`, syntax-pilot readiness, and the real local benchmark if the pinned package is present. Commit only `bench/**`; report SHA, exact commands, raw data paths, sample counts/intervals, and all unavailable baselines. Stop for review.
