# Wave 12: connect memory, foreign calls, notebooks, and proof to source

Six user-owned chats start from the same reviewed `wave12/integration` commit. Each has one bounded coding deliverable and exclusive write paths. Workers commit their paths and stop for the user's review; the coordinator integrates only after the user reports completion. `main` is unchanged. No scheduled continuation or automatic merge is authorized.

| Lane | Exclusive write surface | Deliverable |
| --- | --- | --- |
| [Native memory functions](native-memory-functions/plan.md) | `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Native build/run parity for the exact Wave 11 local memory-function profile |
| [Managed object graph](managed-object-graph/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Additive runtime object references and graph tracing with failure/region invariants |
| [Source native C calls](source-native-c/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**`, `crates/cgx2-package/**`, `crates/cgx2-bind/**`, `crates/cgx2-ffi-native/**` | Explicit unsafe `.cg` calls to one pinned, fixed-shape C ABI profile, without Python |
| [Incremental notebook session](incremental-notebook/plan.md) | `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, `crates/cgx2-notebook-ui/**` | Execute pure scalar cells from retained state without replaying earlier cells |
| [Source contract verification](source-contracts/plan.md) | `crates/cgx2-verify-core/**`, `crates/cgx2-verify-cli/**` | Extract a checked pure `.cg` contract subset into bounded verification with source locations |
| [Benchmark task evidence](benchmark-tasks/plan.md) | `bench/**` | Frozen task-pack validation and honest run metadata for later paired LLM/runtime studies |

## Frozen contracts and coordination

- Workers read `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and their assigned `plan.md` before coding. Follow the AGENTS.md knowledge-graph workflow. Root `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/**`, and `plans/**` are coordinator-owned. A worker may update only its lane paths, with no cross-worker messages, merges, follow-on chats, or automation.
- Every lane consumes **Wave 11 APIs at dispatch**. The native compiler uses the already checked zero-argument Unit local memory functions, not the parallel runtime's new graph API. The graph lane adds API without changing existing `Session` behavior. The notebook lane keeps its own evaluator/session boundary and does not consume source-native changes. The verifier reads existing parser/checker APIs but does not require new syntax. The benchmark lane uses frozen Wave 11 behavior and labels planned tasks separately from measured runs.
- Source native C calls must be explicit `unsafe foreign` and use a preflighted, exact artifact hash and a fixed reviewed call shape. A self-asserted lock cannot grant a safe call or prove a symbol's physical signature. Python, LibTorch, GPU, and arbitrary library calls are outside this lane's acceptance. Direct LibTorch remains the intended PyTorch route and a separate integration gate.
- Each worker runs its targeted offline tests, `cargo fmt --all --check`, targeted Clippy with `-D warnings`, and records exact skips. The coordinator checks ownership, safety and combined behavior, then runs workspace Rust/Python/UI gates. Real LibTorch and a trusted live solver require external artifacts that are unavailable on this host. No LLM speed or release readiness claim follows from a passing component suite.

## Release gates still open

The first release still needs source-level direct LibTorch, full typed/owned memory and foreign lifetimes, persistent notebook effects, broad module/type coverage, trusted proof for any global claim, general parallel/GPU execution, Python-only library access beyond the restricted route, cross-target conformance, and genuine paired LLM/runtime evidence. The hard memory correctness gate remains first even if Wave 12's finite corpus passes.
