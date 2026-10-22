# Wave 7: module graph and release-gate foundations

Five user-owned chats start from the same verified `wave7/integration` commit after Wave 6 integration. This is a tactical code split. Plan for about 15–25% coordinator review and integration time, rather than treating five chats as five times the throughput. Each chat owns disjoint paths, commits its work, reports tests and limits, and stops for user review. No automation, worker-to-worker messaging, or merge to `main`.

| Lane | Exclusive write surface | Frozen input | Reviewable output |
| --- | --- | --- | --- |
| [Memory assurance](memory-assurance/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Wave 6 memory source/runtime API | Conversion-aware seeded model and targeted failure invariants |
| [Package imports](package-imports/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-package/**`, `crates/cgx2-api-index/**` | Wave 6 scalar/public-signature API | Explicit local `import` declarations, resolved acyclic graph, checked public calls |
| [Python value bridge](python-values/plan.md) | `crates/cgx2-python-bridge/**` | Embedded CPython and typed CPU tensor bridge | Bounded keyword/JSON-like value exchange with foreign ownership made explicit |
| [Verification command](verification-command/plan.md) | `crates/cgx2-verify-core/**`, new `crates/cgx2-verify-cli/**` | Existing typed `Contract` and bounded/SMT primitives | Versioned contract input, bounded verdict CLI, honest solver status |
| [Benchmark readiness](benchmark-readiness/plan.md) | `bench/**` | Wave 6 CLI, `cgx2c`, task/result schema | Reproducible task-matrix audit and measured native-scalar pilot without model-win claims |

## One-card interface contract

- The package-import lane is the only lane that changes the source AST. It may add an import collection to `Module` but must preserve `parse_module`, single-file `check_module`, existing public signature APIs, the Wave 6 memory syntax, and scalar CLI behavior. It must run the whole workspace tests and repair its own downstream compatibility within its owned paths; if a different crate requires an edit, report it for coordinator integration rather than editing another lane. Package checks may resolve and type-check calls; `cgx2 run`/`cgx2c build` still reject or omit package execution explicitly.
- Memory assurance consumes the frozen `run_source_with_report` and runtime `Session` APIs. It may repair runtime internals but must not change public method signatures or memory error/collection semantics; package imports cannot modify the memory checker. The Python and verification lanes have no source parser dependency. Benchmark work consumes committed Wave 6 commands and may report a new Wave 7 feature as unavailable until integration.
- Python/PyTorch allocations remain foreign, outside CGx2 region and `no_gc` accounting. A dynamic Python handle does not become a memory-safe CGx2 value by conversion. Real PyTorch tests are optional only if the interpreter linked at build time has the genuine package; skips are recorded as missing evidence.
- Verification results distinguish finite-domain `Proved` from global proof, and solver protocol statuses from trusted proof. No fake solver transcript may be reported as a real theorem. Benchmark outputs distinguish real subprocess observations from model-token traces and from release targets.
- Root `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/**`, `plans/roadmap.md`, and `plans/progress.md` are coordinator-owned. Root `Cargo.toml` already uses `crates/*`; workers do not commit lockfile churn. Source syntax, package graph, runtime, Python bridge, verifier, and benchmark files do not overlap. The coordinator cherry-picks on `wave7/integration`, resolves cross-crate drift, runs combined Rust/Python tests, fmt, Clippy, bridge tests, package/import examples, memory report, verification command, and pilot parity before any next wave.

## Deferred shared-interface work

Running imported functions, source effect/capability/contract declarations, typed whole-program IR, native managed-memory emission, Python calls from `.cg`, persistent side-effecting notebook reruns, and automatic general GPU execution require source/semantic/runtime interfaces that this wave does not safely split across chats. The first-release hard memory gate and LLM/runtime outcome targets remain open. A future wave can use the import graph and verification/benchmark surfaces here as inputs, after their contracts pass review.
