# Reusable checked memory functions

**Goal.** Improve modularity and reduce repeated `.cg` boilerplate by allowing a narrowly typed scalar parameter profile in existing local memory functions. Keep memory handles private to each call and runtime limits authoritative.

**Ownership.** Only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**`, `crates/cgx2-emit-rust/**`, and `crates/cgx2c/**`. Consume the frozen Wave 16 runtime API. Do not edit runtime/conformance, notebooks, verifier, root Cargo files, or shared docs. Changes across owned crates must be integrated in one commit.

## Deliverable

1. Admit direct acyclic local memory-function calls with explicit `i64` scalar parameters and `Unit` result. In this first slice, parameters may supply only the byte-size argument of `memory.alloc_bytes(region, N)` and `memory.owned_bytes(N)`. Keep region limits, `no_gc` budgets, object payloads/reference-slot counts, reference indices, handle arguments/results, recursion, implicit capture, mixed scalar/memory bodies, arbitrary control flow, and foreign calls in their current literal-only or unsupported profiles. Reject unsupported forms with source spans. Public signatures and transitive effects/capabilities remain visible in the API index.
2. Specify and enforce conversion of signed `i64` byte sizes to runtime `usize`: handle zero, negative values, target-`usize` overflow, region-limit failure, and `no_gc` budget failure with stable diagnostic/error class and source span at the dynamic argument. Check call arity/types and effect/capability propagation. Avoid adding a second syntax for LLMs.
3. Run checked `.cg` fixtures through `cgx2 check`, interpreted `run`, generated Rust/native debug and release, and `cgx2 api --json`. Compare result, error class and source span, and memory counters/report snapshots on success and failure; include zero, negative, `i64::MAX`, region-limit and `no_gc` failures. Unsupported package/graph forms must fail clearly rather than silently changing target behavior.

## Verification

Run targeted offline tests for all owned crates, format, Clippy with warnings denied, direct CLI/native fixtures and negative checker cases. Document exact admitted fragment, examples, and remaining exclusions in owned READMEs. Report commit ID and stop for review.
