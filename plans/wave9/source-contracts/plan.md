# Source Effects and Contracts Implementation Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Work directly in this chat. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Make a bounded set of effects and pre/postconditions visible in ordinary `.cg` function source, static diagnostics, runtime execution, and the public API index.

**Architecture:** Extend the existing function AST and scalar checker rather than introducing a second syntax or a separate verifier dialect. Explicit declarations are checked against direct and transitive local calls. The scalar evaluator checks Boolean preconditions before the body and postconditions after a successful return. Every unsupported or unproved case is labeled runtime-checked or unknown, never proved.

**Tech stack:** Rust 1.98, `cgx2-syntax`, `cgx2-sema`, `cgx2-cli`, `cgx2-api-index`; offline Cargo.

**Spec:** `docs/language-design.md` sections “Effects, capabilities, contracts, and proofs” and “Compiler, tooling, and LLM use”; `docs/validation-plan.md`.

## Global constraints

- Own only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**`, and this plan. Preserve old source and command behavior unless an old program is demonstrably unsafe; add regression tests before changing it.
- Keep one readable `.cg` grammar. Proposed bounded form: `fn f(x: i64) -> i64 effects(ReadArgs) caps(ArgsRead):` followed by optional `requires EXPR` and `ensures EXPR` lines before executable body statements. `effects()` and `caps()` mean explicit empty sets. Omitted lists retain legacy inferred behavior for existing code but are labeled inferred in the API. Canonical formatter must round-trip the new form. If the parser has a simpler compatible form, document and test the exact grammar before implementing the checker.
- Effect names for this slice: `ReadArgs`, `ManagedAlloc`, `Collect`, `Mutate`, `Foreign`, and `Unknown`. Capability names: `ArgsRead`, `MemoryCollect`, `ForeignCall`. `arg`/`arg_count` require `ReadArgs` plus `ArgsRead`; `memory.collect` requires `Collect` plus `MemoryCollect`; a syntactically known foreign call requires `Foreign` plus `ForeignCall`. Allocation and assignment are effects but do not grant a capability. Unknown calls do not silently pass an explicit pure signature.
- Contracts use the existing scalar `Bool` result, integer/Boolean parameters, and `result` in `ensures`; extend comparison operators only as needed for useful examples, with checked `i64` behavior preserved. A failed contract yields a source-located nonzero diagnostic and no success output. A contract expression with effects, unsupported memory values, ambiguous `result`, or unbounded recursion is rejected explicitly. Do not call a runtime pass a proof.
- Keep diagnostic format stable: code, severity, message, zero-based half-open UTF-8 byte span. Bound contract/call evaluation by existing step/call limits. A contract may not run side effects or execute inside a `no_gc` body through hidden calls.

## Review focus

- An explicit pure function calling `arg(0)` must fail static checking at the call span.
- A declared effect must not imply a missing capability is available.
- A false `requires` must stop before body effects; a false `ensures` must reject the returned value.
- An indirect or unresolved call in an explicit signature must be diagnosed `Unknown`, not treated as pure.
- Unicode before a failing contract must preserve UTF-8 byte spans and valid JSON diagnostics.

## Tasks

### Task 1: Parse and format declarations

**Files:** `crates/cgx2-syntax/src/{ast,lexer,parser,format}.rs`; parser/formatter tests under `crates/cgx2-syntax/tests/`.

- [ ] Add failing parse/round-trip tests for explicit empty and nonempty lists, `requires`/`ensures`, duplicate or unknown names, public/private functions, and unchanged old fixtures.
- [ ] Add AST fields with spans for declared effects/capabilities and contract expressions; parse only the documented header/prelude form and emit canonical text.
- [ ] Run `cargo test -p cgx2-syntax --offline`; test new and old fixtures.

### Task 2: Static effect and capability gate

**Files:** `crates/cgx2-sema/src/{check,diagnostic,types}.rs`, `crates/cgx2-sema/tests/**`.

- [ ] Add failing tests for direct/transitive effects, declared pure mismatch, missing capability, unknown/imported call, and duplicate annotations. Preserve existing memory/ownership diagnostics.
- [ ] Compute conservative local call effects to a fixed point. Check explicit declarations and capability requirements; mark omitted declarations inferred so old programs continue to run. Bound graph traversal and return a diagnostic on exhaustion.
- [ ] Run `cargo test -p cgx2-sema --offline`.

### Task 3: Runtime contracts and public API

**Files:** `crates/cgx2-cli/src/{eval,check,diagnostics}.rs`, `crates/cgx2-cli/tests/**`, `crates/cgx2-api-index/src/**`, `crates/cgx2-api-index/tests/**`.

- [ ] Add failing CLI tests for true/false pre/postconditions, no body run on failed precondition, result binding, step-limit failure, invalid pure contract call, and JSON/source spans. Add API-index tests for explicit versus inferred declarations and contract summaries.
- [ ] Evaluate contracts only after static validation, using the existing bounded scalar environment; preserve a failure's code/span in `check`, `run`, and JSON output. Index declarations without executing user code.
- [ ] Run `cargo test -p cgx2-cli -p cgx2-api-index --offline` and one `cargo run -p cgx2-cli --offline -- run` contract example.

### Task 4: Compatibility and handoff

- [ ] Run `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check` and targeted Clippy `-D warnings`.
- [ ] Document the exact syntax, runtime-checked status, unsupported proof/memory/foreign cases, and any inherited annotation rule in crate READMEs. Commit only owned files and report a runnable example plus diagnostic example.

**Acceptance:** One `.cg` source syntax exposes explicit effects/capabilities and bounded contracts to humans, the compiler, runtime, and API tooling. A failing contract or undeclared effect cannot be reported as a successful or proved run.
