# CGx2 implementation roadmap: first compiler to first usable release

**Status:** implementation plan; no compiler exists. This replaces the earlier ownership-only v0.1 sequence after the user's confirmed design interview. [Specification](../../language-design.md) · [Validation gates](../../validation-plan.md).

## Definition of done

“First usable release” means the integrated feature contract in the specification is implemented **and** its memory guarantees, conformance suites, and preregistered benchmark gates pass. Intermediate milestones are internal runnable previews, not a reduced public release. Do not rename an incomplete preview “v1.” OCaml-scale ecosystem completeness is a longer-term platform goal.

Every milestone ships a fixture or executable demonstration, human and JSON diagnostics where relevant, a documented unsupported-feature boundary, and a reproducible test command. Memory behavior is specified and tested before performance tuning or syntax minimization.

## Architecture and planned workspace

Use a Rust workspace. The initial compiler pipeline is spanned source → parser → module graph → typed IR with effects/contracts → ownership and region checks → Rust emission. Keep target-neutral semantics in the IR. The native runtime owns managed regions; generated Rust should remain safe where possible. Internal runtime unsafe code receives isolated audit and sanitizer coverage.

| Planned path | Responsibility |
| --- | --- |
| `crates/cgx2-syntax/` | Lexer, indentation/newline parser, AST, source spans, formatter. |
| `crates/cgx2-semantic/` | Module graph, names, types, effects, capabilities, contracts, typed IR. |
| `crates/cgx2-memory/` | Ownership/borrow and region checking; allocation effect summaries. |
| `crates/cgx2-runtime/` | Per-region tracing, stable handles, manual/auto collection, budgets, instrumentation. |
| `crates/cgx2-backend-rust/` | Readable Rust emission and source maps. |
| `crates/cgx2-backend-python/` | Supported portable-subset Python emission and compatibility helpers. |
| `crates/cgx2-python/` | CPython/PyO3 host, typed PyTorch operations, dynamic escape hatch. |
| `crates/cgx2-verify/` | Contract lowering, SMT solver interface, proof result accounting. |
| `crates/cgx2-notebook/` | `.cgnb` parser, persistent session, dependency tracking, outputs/assets. |
| `crates/cgx2-analyze/` | Security checks, profiler integration, performance/resource reporting. |
| `crates/cgx2-cli/` | `cgx2` commands, human/JSON diagnostics, project/script dispatch. |
| `tests/conformance/`, `bench/` | Language and memory fixtures, foreign-boundary tests, benchmark harness. |

This map is a proposed decomposition, not a reason to create empty crates. Create each path when its first vertical slice exists. Preserve stable diagnostic codes and source spans through every pass.

## Milestone 0 — executable specification and measurement harness

**Deliverables**

- Write normative rules for values, regions, safepoints, budget accounting, region exit, owned/managed conversion, errors, foreign boundaries, and notebook cell state. Give valid and invalid examples.
- Define the first grammar candidate and an equivalent brace candidate. Use the same example programs, module APIs, and language features for the syntax comparison.
- Define diagnostic JSON schema, CLI exit statuses, portable target feature matrix, proof subset, and `.cgnb` serialization fixtures.
- Create the six-task pilot and baseline harness using existing languages. Run power/noise analysis before freezing the full holdout and numeric performance gates.

**Gate:** every memory and syntax rule has at least one positive and one negative executable fixture; no release claim is based on a pilot. If the syntax comparison cannot distinguish candidates, choose the simpler grammar under the memory/error-diagnostic constraints and record why.

## Milestone 1 — native script and module compiler preview

**Build**

1. CLI parses a single `.cg` script without a manifest and a package with `cgx2.toml`; `cgx2 run file.cg -- args` and `cgx2 file.cg -- args` have the same argument and exit behavior.
2. Lexer/parser preserve UTF-8 byte spans and recover after common errors. Formatter emits canonical indentation. Reject mixed/tab indentation with precise diagnostics.
3. Resolve explicit imports, check public signatures and local inference, and lower primitives, functions, records/variants, `Option`/`Result`, and checked arithmetic to typed IR.
4. Emit readable Rust and run a two-module native program. `cgx2 check` stops before code generation. Backend failures map to CGx2 diagnostics.

**Gate:** a script prints output and receives CLI arguments; two modules compile and run; invalid import/type/overflow cases get stable CGx2 errors in both human and JSON forms. Generated Rust passes source snapshots and behavior fixtures. This is an internal preview.

## Milestone 2 — memory kernel and safe region semantics

**Build**

1. Default managed region with traced, stable handles and documented safepoints.
2. Named `auto` and `manual` regions with limits; `memory.collect(region)` affects only that region.
3. Owned move/borrow checking, deterministic owned cleanup, explicit managed/owned conversion, cross-region transfer rules.
4. Budgeted `no_gc` with nested/error/cancellation behavior and `AllocationError` propagation; per-region allocation/collection counters and pause traces.

**Gate:** all hard memory fixtures in the validation plan pass. Fault injection at every allocation point and randomized state-machine tests preserve invariants. Instrumentation confirms zero CGx2 collections inside `no_gc`; manual regions never collect implicitly. Runtime internal unsafe code has review and sanitizer runs. Performance measurements are recorded, but correctness cannot be waived for speed.

## Milestone 3 — contracts, effects, capabilities, and tests

**Build**

- Effect inference/checking for allocation, collection, file/network I/O, and foreign calls; public effect signatures and explicit capability parameters.
- `requires`, `ensures`, assertions, loop invariants, runtime contract checks, and `cgx2 test` with example/property tests.
- `cgx2 api --json` exposing exports, imports, types, effects, capabilities, contract summaries, and source locations. Dependency symbols are checked against pinned installed versions.

**Gate:** impure calls in pure functions, missing capabilities, and contract failures are reported with source spans; independent hidden tests catch behavior that compilation accepts. Tool responses give an LLM enough local context to repair a failure without reading an entire repository.

## Milestone 4 — bounded formal verification and analyzers

**Build**

- Verification conditions for the documented pure integer/Boolean, array-bounds, ownership/region, and annotated-loop subset; solver integration with distinct proved/disproved/unknown outcomes.
- Explicit assumptions for Python/PyTorch and other unmodeled effects; proof reports disclose assumptions and runtime-check fallbacks.
- Security checks for effect/capability policy and documented taint patterns; measured profiler and per-region memory report; advisory static performance hints.

**Gate:** known false contracts yield counterexamples, unknown/timeouts never appear as proof, unsupported constructs are diagnosed, and analyzer seeded cases have measured false-positive/miss rates. Memory invariants continue to pass after instrumentation.

## Milestone 5 — Python/PyTorch and generic foreign boundaries

**Build**

- Pin/select an installed CPython environment, embed it through PyO3 for native programs, translate exceptions and tracebacks to CGx2 diagnostics, and specify thread/lifetime rules.
- Provide typed `Tensor` operations for common inference/data code plus an explicitly dynamic `PythonValue` escape hatch; show dtype/device/shape and foreign allocation in tooling.
- Implement a declared C ABI adapter and process/RPC adapter for libraries outside Python, with explicit layout, ownership, error, and serialization contracts.
- Build a reference CLI program that loads real PyTorch, performs an inference or data transformation, and reports correct output on CPU; include GPU fixtures where hardware exists.

**Gate:** the reference app calls an installed PyTorch package without a mock; invalid Python signatures, exceptions, lifetime errors, and missing environments fail clearly. Boundary alias/copy tests pass. CGx2 memory counters do not misattribute Python or GPU memory.

## Milestone 6 — native CGx2 notebook

**Build**

- Versioned readable `.cgnb` parser/writer, code and Markdown cells, stable IDs, text outputs, content-addressed rich assets, and self-contained export.
- Persistent incremental CGx2 session with no automatic replay of earlier side effects; cell rerun, stale dependency marking, and Restart and Run All.
- Dedicated interface with tables, plots, display values, stdout/stderr, errors, and safe rich-output rendering; `cgx2 notebook open|run` CLI.

**Gate:** a PyTorch experiment executes across cells, rerunning a cell does not replay earlier effects, clean Run All reproduces pinned results, changed definitions invalidate dependent outputs, and export reopens without sidecars. Notebook code has the same memory/effect/contract semantics as scripts.

## Milestone 7 — Python source target and portable behavior

**Build**

- Emit editable Python for the declared portable subset, including selected typed PyTorch calls. Use explicit compatibility helpers where checked integer or contract semantics need them.
- Diagnose `no_gc`, manual regions, owned borrow behavior, or other unsupported constructs instead of emitting misleading Python.
- Improve Rust source layout/readability. Add cross-target fixtures and blinded maintenance tasks. Document CGx2 as source of truth; generated edits do not sync back.

**Gate:** Rust and Python output pass the same tests on every supported fixture, boundary/exception behavior is documented, and maintainers can make a specified change in emitted source without reverse engineering opaque glue. Unsupported features produce stable target diagnostics.

## Milestone 8 — integrated release validation

Run all hard memory conformance tests again on the assembled product. Run full CLI, notebook, proof, security, PyTorch, and portability suites. Then execute the preregistered LLM/runtime holdout in [the validation plan](../../validation-plan.md). If a gate fails, fix the implementation or state that the first usable release is not ready. Publish source, benchmark methods, confidence intervals, unsupported target matrix, and known limits. Freeze compatibility and migration policy before a public stable release.

## Beyond the first usable release

Mature the package manager/registry, standard library, REPL/debugger/LSP, profiler, documentation, compatibility process, browser target, TypeScript source output, concurrency/distributed facilities, and broader foreign adapters. Compare platform breadth to the OCaml compiler and ecosystem with an explicit capability checklist, not a repository line-count target. New features need semantic conformance and LLM comprehension tests so breadth does not erode the memory-first design.
