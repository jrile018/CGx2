# CGx2 implementation architecture, Lean assurance, and comparative evaluation

Date: 2026-11-13. Status: confirmed by the user and authorized for parallel execution. See [execution DAG](execution.md). This document authorizes no claim of superiority or completed implementation proof.

## 1. Decisions settled through the grilling interview

| Decision | Agreed direction |
| --- | --- |
| Adoption scope | Evaluate native systems, data/ML, and general/LLM-assisted development separately. |
| Design under review | Intended complete CGx2 architecture, including unfinished aggregate ownership. Do not silently substitute the narrower accepted preview. |
| Evidence discipline | Distinguish intended semantics, unfinished implementation, tested behavior, and proved properties. |
| Unfavorable findings | Explain each concrete result, its limits, and alternatives; the user chooses the response case by case. No global abandon/retain rule. |
| Change freedom | Breaking changes are permitted on isolated branches with migration notes. Keep published main and original unfinished work intact during investigation. |
| Competitors | Rust, OCaml, and Python; compare both idiomatic implementations and implementations equipped with equivalent policies. |
| First proof milestone | Connect the actual runtime budget/guard core to Lean, then expand toward ownership and compiler transformations. |
| Applications | Use the starter suite below, supplementing it when a specific use case exposes a gap. |
| Protected foreign calls | Reject unknown or unjustified foreign allocation/collection behavior inside strict protected blocks. Ordinary interoperability remains available outside them. |
| LLM outcome | Correct completion first; total input/output/tool tokens, time, repairs, and intervention include failed attempts. Compactness is separate. |
| Hardware | CPU and GPU investigation both begin immediately; neither is silently removed when prerequisites fail. |
| First concurrency boundary | Single-threaded managed sessions first, explicit owned worker transfers subsequently. Shared managed heaps require a separate proof milestone. |
| External model spending | No new external spending initially. Prepare the study and a concrete costed proposal before model trials. |

Once this plan is confirmed, proceed autonomously through work that does not require a new substantive choice. At an unfavorable result, unsupported proof route, or material architectural tradeoff, present concrete evidence and alternatives before taking the dependent direction. Routine verification and implementation choices do not require repeated approval.

## 2. Starting evidence and limitations

The inspected committed crate trees at `e50bc3bf28b8aebf7faf8e12564372599feda06b` and proof branch `ad1a91c41d6f5ed192e08750c377cd68cf23de80` match. The original `5185` worktree additionally contains uncommitted ownership v4 and owned-tree work. Main was `02ad9ebb2062cb597a64d9e3e2934bf5f07cad2c` during discovery. Refresh these identities before execution.

The [existing Lean package](../../formal/memory-guarantees/README.md) checks 13 reference-model theorems. It does not connect to actual Rust source, model guard entry/exit, or formalize competing languages. Its entirely-unchanged-state rejection theorem does not directly describe runtime diagnostic updates. Do not promote it to an implementation guarantee.

The actual runtime uses checked and saturating machine arithmetic, host reservations, actual vector capacities, logical metadata charges, fault injection, diagnostic counters, and RAII guard lifetimes. Authentication by reconstruction/digest comparison is valuable but is not a theorem of semantic preservation.

The current collector repeatedly scans slots/edges until marking stabilizes. General ownership source integration remains incomplete. Neither fact alone establishes that the whole architecture should be abandoned or retained.

Discovery found Linux x86-64, Ryzen 7 9800X3D, Rust 1.98.1, Python 3.14.4, and OCaml 5.5.1 in an explicit opam switch. AMD discrete GPU hardware exists, but usable compute tooling/device access was not validated. Historical Python/Torch interpreter pins have drifted, and historical temporary LibTorch packages are absent. Lean and Rust-to-Lean extraction tools were not found. These are prerequisites to resolve, not benchmark results.

## 3. Work packages and dependencies

### W0 — Freeze the intended architecture and reproducible inputs

- Record exact commits, dirty-file hashes, source snapshots, toolchains, and evidence provenance. Preserve original worktrees. Snapshot unfinished inputs consistently before importing selected work into an isolated candidate branch.
- Read the current design, validation plan, roadmap, release ledger, and unfinished ownership plan. Map source checking, specialization, profile dispatch, internal representations, interpreter execution, Rust emission, native compilation, runtime storage, and foreign adapters.
- Create a claim ledger. Every claim identifies its supported program subset, observables, assumptions, implementation files, proof obligation, test evidence, and unresolved gap.
- Review the intended complete design immediately. Missing source capability is an implementation gap; a defective invariant or incompatible design goal is an architectural problem. Never count library-only behavior as completed source support.

Deliverables: revision/input manifest, architecture map, claim ledger, and a dependency map of unfinished features. No performance credit is assigned at this stage.

### W1 — Establish CPU and GPU lanes from the beginning

- CPU lane: pin explicit Rust/OCaml/Python toolchains, build modes, CPU affinity, thread counts, dependency closures, and measurement conditions.
- GPU lane: establish device access and supported driver/runtime/library combinations; run a genuine device smoke test before any GPU result. Record device identity, backend, actual execution device, precision, and synchronization. Detect and report CPU fallback.
- Rebuild or validate a coherent Python/Torch environment and the native LibTorch facade/closure. Distinguish native calls from calls through Python and verify relevant loaded-library claims.
- Keep host managed-session semantics separate from GPU streams, asynchronous completion, device buffers, and foreign library threads.
- If GPU prerequisites cannot be established, report the exact blocker and feasible remedies. Continue independent CPU/formal work, but retain GPU as an open, blocking evaluation row. Do not publish a CPU-only conclusion as a combined CPU/GPU result. Prepare any required environment change concretely before requesting user action.

Deliverables: two environment manifests, reproducible smoke tests, and explicit readiness/blocker reports. GPU benchmarking is required where the workload meaningfully uses a GPU; CPU-only workloads receive a stated applicability explanation.

### W2 — Prove the implementation-connected runtime core

First run a bounded feasibility spike for a pinned Rust-to-Lean route, such as Charon/Aeneas, against the actual admission/charge kernel. Determine supported operations and versions before promising extraction coverage. Record the trusted frontend, translation, libraries, Lean kernel, compiler, and host assumptions. An unverified translator remains part of the trusted boundary.

Distinguish these evidence levels in every report:

1. A hand-written reference model with checked theorems.
2. Proofs over actual-source translations, with explicit translation and environment trust.
3. Compiler-pass/source-to-runtime preservation claims, only where separately established.
4. Differential/conformance tests, which establish agreement for tested traces rather than all-input equivalence.

Do not silently replace an unsupported source-verification route with a hand-copy and retain the same claim. Present the exact obstacle and choices: a semantics-preserving kernel refactor, a different verification route, or a clearly weaker interim result.

Initial proof obligations:

| Component | Obligation |
| --- | --- |
| `check_budgets` | Admission matches the intended condition under reachable valid states; every active guard participates; rejection does not commit charges. |
| `charge_budgets` | Given admission for identical bytes and unchanged guards, `usize` addition cannot overflow and each budget remains valid. Model saturating telemetry explicitly. |
| Allocation preflight/commit | Checked arithmetic, region checks, actual-capacity charges, and final rechecks preserve invariants; no handle is published before initialization. |
| Protected failure | No published payload/handle or successful allocation charge is committed on rejection. Permit specified diagnostics; distinguish temporary host reservations and RSS from logical budget charges. |
| Guard entry | `try_no_gc` charges outer guards for permitted bookkeeping growth, creates one zero-spend inner guard, and preserves existing guards on failure. |
| Guard exit | Stack/lifetime relation covers normal return, error and unwind. Arithmetic proofs alone do not prove RAII behavior. |
| Entry-point coverage | Review compatibility `no_gc` separately; proving `try_no_gc` does not cover another entry point automatically. |
| Counters | Prove safe updates or state explicit bounds. Projecting diagnostics out of an observation relation does not justify ignoring an overflow that changes control flow. |

The runtime's 64-byte logical slot charge is not a theorem about all allocator overhead or process RSS. Collection exclusion is not a bound on total execution latency. Prove precisely stated properties, not stronger marketing interpretations.

Deliverables: reproducible checked source connection, observation relation/event model, theorem and axiom inventory, arithmetic/failure/guard obligations, and fault/boundary trace replay. If translation is blocked, the deliverable is an explicit feasibility finding, not a completed implementation proof.

### W3 — Review ownership and compiler architecture against the complete design

- Model affine aggregate moves, partial initialization/drop, projections, shared/exclusive loans and reborrows, region lifetimes, generic layouts, branches/loops, recursive/package summaries, and authenticated source attribution.
- Check the unfinished v4/tree implementation against these obligations. Track required source integration explicitly; do not substitute an analyzer API or runtime data structure for end-to-end accepted source behavior.
- Establish pass-specific semantic relations through rewriting, checking, specialization, lowering, interpretation and Rust emission. Recomputed hashes do not establish preservation.
- Evaluate whether multiple profile routes enlarge the trusted surface unnecessarily, and whether a smaller common representation/core would reduce inconsistency and proof effort.
- Audit roots, stable handles/generations, cross-region transfer, exactly-once cleanup, error/unwind/cancellation paths, and host/foreign assumptions in stages.
- Enforce the strict foreign boundary at the source/effect and execution interfaces. A foreign annotation alone is not a proof; any audited external contract remains an explicit assumption unless actually verified.

Deliverables: intended-semantics specification, ownership/source gap matrix, pass-preservation obligations, and concrete architecture alternatives. Implement material redesign choices only after explaining the actual tradeoff to the user.

### W4 — Build fair native, ML and general application comparisons

| Track | Initial workloads | Required observations |
| --- | --- | --- |
| Native | Owned-buffer pipeline; managed object graph; allocation-heavy work surrounding a protected critical section | Correctness, failure semantics, allocation charges, collection activity, latency distribution, throughput, RSS, implementation effort |
| Data/ML | Tensor inference plus preprocessing; foreign-call/copy pipeline; persistent notebook workflow | Equivalent outputs/tolerances, cold/warm costs, copies, native/Python boundary, host/device memory, transfers and synchronization |
| General/LLM | Multi-module records/variants/collections/error application; bug fixes; refactoring tasks | Functional acceptance, diagnostics, API complexity, change effort, source size, later controlled authoring outcomes |

For each meaningful workload, implement both idiomatic baselines and equally policy-equipped baselines. Match semantics, supported inputs, output/error behavior, external dependencies and resource constraints. Include the effort required to add the policy; do not hide it, and do not attribute it automatically to a language limitation.

Rust without a tracing collector already avoids that collector's pauses. A budget wrapper in another language may enforce the same bound. Proving CGx2's policy alone cannot establish a unique language advantage.

For GPU cases, pin equivalent backend kernels, shapes, dtype, precision and tolerances. Separate shared-library compute from language orchestration. Include host/device transfers, synchronization, startup, adaptation, fallback and memory lifetime costs. Record unsupported bindings as integration gaps rather than inventing equivalent native support.

Define logical cost counters for the modeled algorithms, and separately measure wall time/resources with repetitions and uncertainty. Use pilots to identify relevant effect sizes; agree on workload-specific practical thresholds before holdout evaluation. Do not move the threshold after seeing holdout results. Preserve unfavorable, mixed and inconclusive observations.

Deliverables: runnable fixtures and baselines, policy contracts, benchmark protocol, raw results with pins, and per-workload comparison scorecards. No aggregate winner is inferred by silently weighting unrelated tracks.

### W5 — Optimize demonstrated weaknesses with proof obligations

Investigate, rather than preselect, these candidates:

- Collector worklist versus repeated whole-graph scans. Count physical slots, reachable nodes and edges. A worklist marking bound of O(V+E) does not remove an O(S) sweep over physical slots. Preserve reachability, handle/root behavior and cleanup semantics.
- Explicitly account for worklist storage or additional object metadata. Preserve protected-block restrictions and collection failure behavior. A faster collector with extra memory or different budget admission needs a user-visible tradeoff and migration account.
- Reduce duplicated semantic/checking routes where this measurably reduces trusted surface or inconsistent behavior.
- Simplify ownership representation or bookkeeping only with corresponding move/loan/lifetime and partial-drop obligations.
- Reduce foreign copies, buffer lifetime overhead or unnecessary synchronization when equivalent semantics and error handling are preserved.

For each proposed optimization: state the observed problem, candidate mechanism, invariant/cost obligations, expected tradeoff and uncertainty; then obtain the user's choice where substantive. Prove the relevant property and validate the executable implementation. Repeat targeted conformance and comparisons. Retain an optimization only for a defensible measured or specified benefit; do not optimize toward a predetermined winner.

Deliverables: isolated candidate commits, checked obligations, before/after results, migration notes, and retained/rejected rationale. No automatic merge to main.

### W6 — Prepare the correctness-first LLM study without new spending

- Prepare matched bug-fix, authoring and refactoring tasks with independent acceptance tests, equivalent APIs/documentation budgets, clean workspaces and no future-history leakage.
- Predeclare correctness-first scoring, failure budget charges, all-task cost-to-success, input/output/tool tokens, time, repair count and human intervention. Report source/output compactness separately. Human readability/maintenance remain additional evidence, not Lean theorems.
- Design paired, randomized runs and uncertainty reporting; distinguish pilot findings from sufficiently powered held-out results. Plan multiple model families and frozen versions for later real trials.
- Provide the concrete protocol, model/resource options and cost estimate before externally funded trials. Do not spend or fabricate model observations in the initial phase.

Deliverables: blinded study package, reference acceptance harness, preregistered scoring/protocol and a costed trial proposal. LLM productivity remains unresolved until genuine trials run.

## 4. Explain results before asking for a direction

Every substantive decision report contains the exact workload/design choice, observed result, proof strength, assumptions, likely cause, options, consequences and recommendation. Distinguish:

- Favorable: an established contract or meaningful measured benefit for the stated case.
- Unfavorable: a failed invariant, concrete excess cost or missing capability relevant to that case.
- Mixed: a stronger contract or reduced programming effort accompanied by another cost.
- Inconclusive: inadequate evidence, uncertainty or an unavailable prerequisite.

Examples to explain rather than flatten into a global verdict:

| Finding | What it means | Directions to present |
| --- | --- | --- |
| Same guarantee as Rust with greater memory use | No demonstrated guarantee advantage; ergonomics or another property may still matter | Reduce overhead, narrow the claim, or retain the tradeoff for a justified use case |
| Worklist speeds collection but adds metadata | A time/space tradeoff with possible changed budget admission | Preallocated scratch space, intrusive metadata, constrained alternative, or retain current collector |
| GPU kernels match but CGx2 copying costs more | The shared backend works; integration/orchestration is the issue | Buffer reuse, batching, boundary redesign, or limit claims to suitable workloads |
| Shorter generated code but more repairs/tokens overall | Compactness improved, authoring productivity did not | Improve diagnostics/API/model support, revise syntax, or reject the productivity claim |
| Proof route cannot translate a core operation | The intended source-verification claim is blocked | Refactor a small kernel, change tools, or explicitly accept a weaker interim result |

An incorrect implementation is never a performance win. Absence of evidence is not proof of superiority or inferiority.

## 5. Execution milestones and completion criteria

1. **Architecture and feasibility checkpoint:** W0 plus CPU/GPU readiness and extraction feasibility from W1/W2. Present actual blockers and alternatives, not another open-ended plan.
2. **First rigorous implementation milestone:** the budget/guard kernel with explicit source correspondence and trust boundaries, executable conformance evidence, and the first relevant baseline cases. GPU work remains active; blocked GPU rows remain open.
3. **Design comparison milestone:** unfinished ownership/compiler gap analysis and complete initial native/ML/general comparison fixtures and scorecards. Genuine LLM-run conclusions remain gated on the later trial proposal.
4. **Optimization milestone:** selected changes, their proofs and measured tradeoffs, with preserved historical evidence and migrations.
5. **Reviewable handoff:** exact commit IDs, reproduction commands, source/tool hashes, theorem/axiom coverage, tests, raw measurements, unsupported cases and unresolved decisions. Merge/publication to main remains a separate user decision.

The initial phase is complete only when its promised evidence or a concrete, explicitly reported blocking finding is delivered. This is a staged research/engineering program, not a promise of a finite universal superiority proof or a calendar ETA before feasibility work.

## 6. Independent checks

- Lean checks pass on the pinned toolchain; no admitted proofs or unreported custom axioms. Mutations that break specified invariants fail the expected proof obligations.
- Extraction inputs/outputs are pinned and reviewable; generated artifacts are not edited to make proofs pass. Manual models and tests retain their weaker labels.
- Valid/invalid source fixtures, debug/release parity, fault injection and relevant ownership/region/runtime checks pass for admitted features. Failed mandatory positives remain failures, never successful negative tests.
- CPU/GPU results identify actual execution paths; missing devices, dependencies or bindings are explicit. No unsupported CPU fallback is counted as GPU success.
- Comparative correctness precedes timing; raw outcomes, variance and failed attempts are retained. No whole-language conclusion is inferred from a narrow microbenchmark.
- Original unfinished work, main, historical logs and source attribution remain intact. New work is committed by coherent milestone on isolated branches for review.
