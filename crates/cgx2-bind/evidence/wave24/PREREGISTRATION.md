# Wave 24 pinned loader experiment

Registered before collecting measurements on base
`425cb0a5467d98b2cf9565be19067b2f39f23fff`, Rust 1.98.1, release profile,
offline locked dependencies, default features. Private build/temp/package paths
are under this worktree's `target/wave24-loader`; no other lane's binaries
will be overwritten.

Use a regular-file copy of `/tmp/cgx2-wave23-boundary-f32`, rewriting only
the copied closure's absolute package paths. CPU Torch 2.11.0, f32, matrices
`[[1,2,3],[4,5,6]]` and `[[7,8],[9,10],[11,12]]`; independent expected result
`[[58,64],[139,154]]`. Require atol=rtol=1e-5, identical stdout before/after,
zero exit status, and no Python in the exact measured ELF or observed maps.

Fixed order: five frozen-base process-cold runs; diagnostic tracing baseline
(five runs); any isolated hash-buffer experiments; five candidate traced runs;
five candidate untraced runs. Runs are sequential, fresh processes, with
OMP_NUM_THREADS=MKL_NUM_THREADS=OPENBLAS_NUM_THREADS=1, no LD_PRELOAD/LD_AUDIT,
LD_LIBRARY_PATH naming only the private pinned package. Timeout is 120 s per
process. Record every failure/timeout; do not replace failed samples. Stop a
performance comparison if correctness or provenance fails. Do not drop caches:
"cold" means a fresh loader/process, not cold filesystem caches. Fixed block
order allows host drift; report observations without a general speed claim.

Save raw stdout/stderr, argv, environment, wall intervals, input/package/toolchain/
binary hashes and diagnostic nested phase events. Readelf, map reads, full pinned
hashing, preflight, and preloaded/loaded verification get directly timed scopes;
scopes overlap, so never add inclusive scopes. Native compute, cleanup and CLI
parsing are unmeasured separately. Observe maps in an additional diagnostic run,
excluded from timing comparisons. Save ldd and readelf for each measured binary.
Report medians/ranges only, no p95 from five samples.

Only try a behavior-preserving optimization supported by these observations.
Full-content verification remains at every original boundary; metadata-only
or global caches, skipped rehashes, and changed failure order are forbidden.
If no useful safe improvement is demonstrated, retain opt-in instrumentation
and report the measured blocker.
