# CGx2 memory policy: Lean reference proof

Lean 4.19.0 checks the theorems in [NoGc.lean](NoGc.lean). They prove properties of an executable mathematical model of a CGx2 allocation policy. They do not establish that CGx2 is universally better than another language, nor that its current compiler/runtime implements this model correctly.

## What is proved

For any finite list of allocation and collection attempts, starting with valid budgets:

| Theorem | Guarantee |
| --- | --- |
| `execute_valid` | Every active guard's cumulative charged bytes stay within its budget. |
| `execute_limits` | Execution never silently increases or replaces any guard's limit. |
| `execute_no_gc` | If at least one guard is active, the collection count never changes, including after handled failures. |
| `rejected_allocation_unchanged` | If any guard cannot admit an allocation, the attempt returns an allocation error with the entire state unchanged. |
| `budget_separation` | A concrete allocator without budget enforcement admits a request that violates the bound; the guarded policy rejects it. |
| `gc_separation` | A concrete allocator without collection suppression performs a collection; the guarded policy rejects it. |

Supporting lemmas establish admission equivalence, guard retention and per-step invariants. Boundary examples cover an exact fit, inner and outer budget exhaustion, cumulative charges surviving failures, and collection outside every guard. The induction theorems quantify over arbitrary trace lengths; they are not bounded enumeration tests.

`unguarded` is an explicitly defined comparison policy, **not a formalization of Rust, OCaml, or Python**. The separation results show the benefit of enforcing this particular contract relative to omitting it. They cannot be generalized to every implementation or configuration of another language. Another language can implement the same policy and obtain the same model guarantees.

## Reproduce

Use the release named by [lean-toolchain](lean-toolchain), with no Mathlib or third-party proof dependencies:

```sh
python3 formal/memory-guarantees/verify.py --lean /path/to/lean-4.19.0/bin/lean
```

With Lean already on `PATH`, omit `--lean`. The checker requires version 4.19.0, runs Lean with warnings as errors, checks for admitted proofs and audits the exported theorem axioms. It also verifies that removing budget checks, ignoring nested guards, allowing protected collection, and committing bytes on failure each cause Lean verification to fail. Ordinary Lean foundational axioms such as `propext` and `Quot.sound` are permitted; no runtime correctness axiom is introduced.

[checked.json](checked.json) records the checked source hash, toolchain, axiom output and negative results. The toolchain used for this check was unpacked under `/tmp`, without changing system packages or the Cargo workspace. A proof-only checkout can reproduce the command with its own Lean installation.

## Model boundaries

This follows the intended contract in [language design](../../docs/language-design.md) and the [validation plan](../../docs/validation-plan.md). A state has a fixed list of active guards, total committed allocation charges and a collection count. Each guard has a natural-number limit and cumulative spend. Allocation charges **every** active guard and commits only after all checks pass. `execute` permits callers to handle a rejected attempt and continue; fail-fast executions are represented by prefixes.

The request's byte count is assumed to include every charge required by the runtime. Natural-number arithmetic does not model machine integer overflow. Zero-byte requests are permitted. Actual host allocation failure, region limits, guard entry/exit, cancellation, threads, object references, ownership, lifetime safety and foreign allocators are not modeled. This is not a bound on RSS or foreign Python/PyTorch memory. No allocator or collector work is timed.

## What a defensible language comparison still requires

1. Prove a refinement from supported CGx2 source, through lowering, to the actual runtime behavior, including checked arithmetic, overhead accounting, guard lifetimes and failure paths. Until then, the result applies to this reference model.
2. Define a named competing implementation and comparable workloads, then formalize the exact contract or cost being compared. Rust ownership, an OCaml GC configuration, and Python's runtime each need their own relevant semantics and boundaries.
3. For speed or memory claims, justify the cost model and validate it on real workloads. For readability, model token use, and development time, use the controlled experiments in the validation plan. Those measurements do not follow from this memory proof.

A reasonable eventual claim is: “For this supported execution model and stated boundary, CGx2 enforces a checked allocation budget and excludes collector work inside a protected block.” A blanket claim that it is faster, safer in every respect, easier to use, or better for every program is not established.
