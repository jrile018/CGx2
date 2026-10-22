# Wave 11: memory integration and release gates

Six user-owned chats start from the same verified `wave11/integration` commit. The work is parallel because the six write surfaces are disjoint and consume Wave 10 interfaces as frozen at dispatch. Each chat commits its owned paths and stops for user review. Budget 20–25% of the wave for coordinator review and integration; adding more chats before the interfaces settle would add more coordination than useful progress.

| Lane | Exclusive write surface | Bounded result |
| --- | --- | --- |
| [Memory in functions](memory-functions/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**` | One checked local function memory profile sharing the existing runtime session, with no handle escape |
| [Portable native compiler](native-compiler/plan.md) | `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Native memory build works after moving the compiler from its source checkout; release profile optimizes runtime and program |
| [Runtime assurance](runtime-assurance/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Failure/collection/handle state-machine v5 and dynamic-check readiness for existing public API |
| [Pinned native bindings](native-bindings/plan.md) | `crates/cgx2-package/**`, new `crates/cgx2-bind/**` | Deterministic package declaration/lock and offline `bind check`, without loading untrusted code |
| [Solver proof protocol](solver-proof/plan.md) | `crates/cgx2-verify-core/**`, `crates/cgx2-verify-cli/**` | Bounded solver result/model validation and honest proof/unknown status for the existing external contract subset |
| [LLM holdout statistics](llm-evidence/plan.md) | `bench/**` | Task-clustered paired intervals and failure-inclusive cost-to-success reports, with no invented model traces |

## Frozen interface and dependency gate

- All workers read `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and their assigned plan before coding. Follow AGENTS.md knowledge-graph overview/query/record. Root `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/**`, and `plans/**` are coordinator-owned. No worker edits another lane, merges branches, starts follow-on work, messages another worker, or automates anything.
- The memory-functions lane may edit source grammar/checker/CLI but uses the existing `cgx2-runtime::Session` API. The runtime-assurance lane preserves that public API. The native-compiler lane works on the **Wave 10 memory script profile** and must not depend on the new function syntax or checker changes in this wave. The binding lane treats native FFI as an unsafe host API and does not make new `.cg` native calls. The proof lane accepts the existing external contract JSON, not new source syntax. The evidence lane treats executables as black boxes.
- Before integration, each lane runs the exact targeted offline tests, format and Clippy checks in its plan and commits only its owned paths. The coordinator reviews path ownership, memory and foreign safety, integrates on an isolated branch, runs the full Rust/Python and UI checks, and reports unavailable real LibTorch/solver/model gates. `main` remains untouched.

## Claims kept open

This wave does not establish full typed managed objects, source-level LibTorch, arbitrary Python library permission, no-replay side-effecting notebook execution, global SMT proof, general GPU execution, all ten ecosystem routes, or an LLM speed advantage. A first-release label still requires every feature and validation gate in the shared design; finite memory tests remain evidence, not proof of universal safety.
