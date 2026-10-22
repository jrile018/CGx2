# CGx2 first-wave workstream contracts

This ledger defines six **independent first-wave coding deliverables**. Each has its own plan and worktree. All start from the same reviewed base commit. The user reviews each completed branch; no worker merges or starts its next task automatically.

## Why six

The first wave implements components that can be tested without a compiler-wide typed IR or a shared root Cargo workspace. A seventh lane would need a still-changing interface or would duplicate another lane's work. The critical path after this wave is to integrate syntax, diagnostics, semantics, runtime, and native code generation; that is a later review decision.

| Track / plan | Exclusive write surface | Produces | Current external limitation |
| --- | --- | --- | --- |
| [Syntax](syntax/plan.md) | `crates/cgx2-syntax/**` | Indentation lexer/parser, source spans, canonical formatter, grammar fixtures | Syntax is provisional pending measured comparison. |
| [Memory](memory/plan.md) | `crates/cgx2-runtime/**` | Standalone region/handle runtime, auto/manual collection, global `no_gc` budget, safety tests | Compiler ownership checker comes later. |
| [Notebook](notebook/plan.md) | `crates/cgx2-notebook-format/**` | Native `.cgnb` parse/write and asset format with round-trip tests | Live cell execution/UI comes later. |
| [Python bridge](python-bridge/plan.md) | `crates/cgx2-python-bridge/**` | CPython host and typed/dynamic call boundary with tests | PyTorch is not installed in the current Python environment; real torch test remains pending. |
| [Benchmarks](benchmarks/plan.md) | `bench/**` | Reproducible baseline/pilot runner and result schema | No CGx2 compiler yet; no superiority claim. |
| [Verifier](verifier/plan.md) | `crates/cgx2-verify-core/**` | Typed verification conditions, SMT-LIB emission, solver result protocol | No `z3` executable here; external proof integration remains pending. |

## Index-card contracts

All tracks **consume** the user-approved [language specification](../docs/language-design.md) and [validation plan](../docs/validation-plan.md). They **produce** standalone crates or a standalone benchmark harness with tests and a small public API documented in their own README. The internals of each component are private to its owner. They do not depend on each other's unmerged branch. The coordinator alone owns the shared specifications, root manifests, integration branches, and release judgment.

Each plan names exact APIs and fixtures for the first slice. Where a later compiler integration will need an adapter, the worker returns the current API and does not change another track to fit it. A candidate syntax or foreign-library fallback is reported as provisional, not silently promoted to a language-wide decision.

## Merge and review protocol

1. Worker runs the exact verification command in its plan and commits the bounded deliverable in its own worktree.
2. Worker stops and reports status, changed paths, test evidence, commit ID, unresolved limits, and any needed contract change in its own chat.
3. User reviews each branch. This coordinator chat does not merge, message workers again, or schedule more work unless the user asks.
4. At the user's request, the coordinator reviews each approved branch, integrates in an explicit order, runs cross-track tests, and proposes the next wave.

## Coordination cost and dependency map

Estimated first-wave coordination is roughly 15–25% of work time: plan/contract drafting, six reviews, and later integration. The same-directory alternative risks concurrent edits to root manifests and source contracts. Six isolated worktrees keep write overlap at zero. The only shared decision left in flight is final syntax choice, so the syntax lane must implement a **candidate** and the benchmark lane must preserve comparable brace samples; neither declares the grammar final.

~~~text
syntax ───────┐
memory ───────┤
notebook ─────┤
python bridge ┼──> reviewed integration (later user-requested step)
benchmarks ───┤
verifier ─────┘
~~~

No first-wave coding track has a hard dependency on another first-wave track. Later compiler, emitter, notebook execution, and analyzer tasks depend on verified outputs and therefore are not dispatched in this wave.
