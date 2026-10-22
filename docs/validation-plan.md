# Validation plan: memory guarantees and measurable LLM value

**Status:** preregistration draft, 2026-10-22. Limited scalar/runtime pilots have been measured; no release-level CGx2 benchmark result has been established. Numbers below are proposed gates to calibrate with baseline pilots before the final holdout is opened. [Language specification](language-design.md).

## Claims and priority

The release decision is ordered:

1. **Memory correctness:** safe CGx2 has no invalid memory access in the supported semantics, manual regions never collect without a command, and `no_gc` never performs a CGx2 collection. A failure blocks release regardless of other scores.
2. **Functional correctness:** first usable features pass conformance, integration, notebook, proof, and cross-target tests.
3. **LLM development value:** matched tasks show comparable verified completion and less model time/tokens.
4. **Runtime value:** native and PyTorch workloads meet calibrated performance/resource targets.

Do not exchange a memory guarantee for shorter syntax or faster model output. Report failed gates and uncertainty rather than aggregating everything into one score.

## Before CGx2 exists: hypothesis pilot

Use existing Python, Rust, TypeScript, and OCaml tooling to pilot a small fresh set of six independently reviewed tasks. Test whether structured source-located diagnostics, a public API index, contracts, and separate acceptance tests improve model repairs. This pilot is for task quality, noise, and power analysis. It is not evidence that CGx2 wins. Freeze benchmark design, language docs, task templates, and failure categories before the full holdout.

## Memory conformance and performance

Test both `auto` and `manual` regions, owned moves/borrows, explicit conversions, nesting, error paths, callbacks, foreign handles, and notebook cell reruns. Use deterministic fixtures, property-based state-machine tests, fuzzing, fault injection, and runtime instrumentation. The checker/runtime must meet these hard gates:

| Guarantee | Pass condition |
| --- | --- |
| Safe memory access | No observed use-after-free, double-free, out-of-bounds access, or borrow escape across the supported conformance and fuzz corpus; all deliberately invalid fixtures are rejected or return defined errors. A finite test suite does not prove absence of all bugs, so publish coverage and audit findings. |
| Manual collection | Collection counter for a manual region changes only after an explicit `collect` call, including when allocations fail. |
| `no_gc` | Exactly zero CGx2 collections in the protected region over every success, failure, nested, and cancellation path. Budget exhaustion returns `AllocationError`; it does not collect or silently increase the limit. |
| Region isolation | Collecting/closing one region cannot corrupt handles in another; illegal cross-region escape is rejected. |
| Foreign boundary | Python-only host allocations and direct LibTorch tensor/device allocations are labeled foreign; CGx2 never claims control of their collectors or device allocator. Explicit handle release and buffer aliasing are checked. |

Publish allocation throughput, live/peak bytes, RSS, collection frequency and pause histogram, out-of-budget rate, and owned/managed conversion cost for fixed workloads. For comparable allocation-heavy managed programs, the **provisional** resource target is peak RSS no more than 2× a tuned OCaml implementation and throughput at least 70% of that baseline; report latency distributions and revise targets only before opening the holdout. For the native owned path, the provisional target is median runtime no more than 1.25× optimized Rust and peak RSS no more than 1.5×. These ratios are workload-specific engineering targets, not properties of the design. Memory correctness gates above do not wait on performance calibration.

## LLM authoring, comprehension, and syntax

Build fresh, language-neutral tasks in two repository tiers: 3–10 files and 20–100 files. Each task has equivalent independently reviewed implementations or starter repositories in CGx2, Python, Rust, TypeScript, and OCaml where applicable. Include feature work, bug repair, API use, memory-sensitive changes, numerical edge cases, Python/PyTorch integration, and code comprehension. Pre-register the strongest applicable baseline per task category rather than choosing a favorable comparison afterward.

The task suite measures:

| Question | Metric |
| --- | --- |
| Can an LLM understand the code? | Top-1/top-3 bug location, correctness of behavior/invariant/change-impact answers under a blinded rubric, time and tokens to answer. |
| Can it write the code with fewer tokens? | Output tokens under each actual model tokenizer, total input+output+tool/diagnostic tokens, non-whitespace source bytes, semantic units such as AST nodes, and human readability ratings. |
| Can it finish reliably? | Hidden-test verified completion, first-pass correctness, repair cycles, wall time to verified completion, human interventions, and regressions. |
| Does the syntax help? | Run equivalent indentation/newline and brace variants with identical APIs, examples, model, tools, and tasks. Compare correctness first, then output/total tokens, repair effort, time, and blinded human comprehension. Freeze the better candidate only after this comparison. |
| Which tool matters? | Ablate structured diagnostics, API lookup, contracts, and dependency validation individually after a usable implementation exists. |

Use at least two model families and repeated paired runs with frozen versions, prompts, tools, dependency locks, documentation budgets, hardware, time limits, and task order randomization. Hide acceptance tests and gold changes from model workspaces; strip future repository history. Independent reviewers adjudicate correct alternatives and ambiguous tasks. Public benchmark tasks may be used for secondary reporting, but fresh holdout tasks support the language claim. A six-task pilot is not a statistically adequate release test. Begin with an independently reviewed 60–100 task holdout, then size or extend it from pilot variance so the predeclared confidence bounds are informative.

**Proposed LLM release target:** verified completion is within five percentage points of the strongest applicable baseline, with the lower 95% confidence bound above the predeclared noninferiority margin. On paired tasks completed correctly in both languages, median total model tokens **and** wall time are each at least 20% lower, with paired 95% confidence intervals excluding no improvement. Report all-task cost-to-success separately, charging failures the full budget, so conditioning on successful tasks cannot hide failure cost. Compare output tokens separately from total tokens. Python must be reported separately from Rust/TypeScript/OCaml because syntax and library familiarity differ. No “LLM optimized” superiority claim is made if these targets fail.

## Runtime, interoperability, and output targets

Use behaviorally equivalent reference outputs before timing. Pin compiler, Python, torch, dependencies, hardware, threads, device, dtype, batch size, input distribution, precision, and warmup. Report cold start separately from warm execution; use repeated independent process runs and uncertainty intervals. For GPU work, synchronize at request boundaries or use device events before reading latency. Measure process RSS and PyTorch allocated/reserved device memory separately.

For automatic CPU/parallel/GPU selection, compare each supported backend with the sequential reference on the same input distribution. Include launch, transfers, synchronization, fallback, and planner overhead in end-to-end latency. Validate exact integer results and documented floating-point tolerances, effect order, ownership and `no_gc` behavior before accepting a speedup. Report selection accuracy and regressions across small and large inputs and across devices; an automatic choice is disabled when its correctness or benefit is uncertain. A source-independent policy prototype is not evidence that arbitrary `.cg` code runs in parallel or on a GPU.

| Workload | Baseline | Provisional target |
| --- | --- | --- |
| Native pure/owned CPU | Optimized Rust implementation | CGx2 median runtime ≤1.25× and peak RSS ≤1.5×. |
| Direct LibTorch compute-dominant inference | Hand-written LibTorch and direct Python using compatible pinned torch/LibTorch builds, with identical device, dtype, inputs, and synchronization | CGx2 throughput ≥90% and p95 end-to-end latency ≤1.15× versus each applicable baseline; report both separately. Confirm no Python runtime is loaded. |
| Checked native C ABI call | Equivalent hand-written native C ABI wrapper | Preliminary median tiny-call latency ≤1.25× after calibration; report correctness, copies, version mismatch rejection, and uncertainty. |
| Tiny Python bridge call | Equivalent hand-written PyO3 bridge | CGx2 bridge overhead ≤1.25×. |
| Generated Rust/Python source | Direct target-language implementation | Same specified outputs/errors on supported profile; readability/editability assessed by blinded maintainers and change tasks. Unsupported features must fail emission clearly. |

These targets are provisional until baseline noise and workload mix are measured. Do not publish p99 latency based on too few observations or compare asynchronous GPU operations without synchronization. Benchmark `cgx2 check`, build, run, and notebook cell warm/cold latency separately. Record compiler/tool versions and generated source hashes.

## Verification and product acceptance

- **Proof suite:** known-valid contracts prove, known-false contracts produce counterexamples, timeouts and unsupported Python calls report `unknown`/assumptions. No runtime test or unchecked foreign contract is labeled a formal proof.
- **Security suite:** capability violations and the documented taint source-to-sink patterns are detected with source spans; measure false positives and missed seeded cases. Do not claim a general vulnerability proof.
- **Notebook suite:** clean Run All is reproducible under pinned inputs; rerunning a cell does not replay previous side effects; changed dependencies mark outputs stale; saved text and referenced rich assets reopen; self-contained export reopens without sidecars.
- **CLI suite:** `cgx2 run file.cg -- args` and `cgx2 file.cg -- args` pass arguments and exit codes correctly; project packages work; human and JSON diagnostics refer to CGx2 source.
- **Portability suite:** supported Rust/Python emitted code passes the same boundary fixtures for arithmetic, strings, errors, contracts, and supported typed PyTorch calls. The direct LibTorch runtime route and optional Python source translation are tested separately. Target-specific constructs get a diagnostic, never a plausible but behavior-changing translation.
- **Documentation suite:** a new developer and a coding agent can find the memory rules, API, effect requirements, and unsupported target features without consulting compiler internals.

## Reporting rules

Publish task definitions where possible after holdout retirement, exact versions, prompts, allowed tools, hardware, exclusions, baseline selection, failures, confidence intervals, and pre/post-calibration changes. Separate “implemented,” “conformance passed,” “benchmark target met,” and “long-term goal.” Runtime speed, memory use, LLM writing speed, and human readability are distinct outcomes. The official [LLVM benchmarking guidance](https://llvm.org/docs/Benchmarking.html), [PyTorch benchmark utilities](https://docs.pytorch.org/docs/stable/benchmark_utils.html), [PyTorch CUDA timing notes](https://docs.pytorch.org/docs/stable/notes/cuda), and the [SWE-bench Verified audit](https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/) motivate measurement and task-quality controls.
