# Wave 13: source memory graphs and measurable release foundations

Six user-owned chats start from the same reviewed `wave13/integration` commit. Each implements one bounded result on exclusive paths, commits it in its attached worktree, and stops for the user's review. The coordinator integrates only after the user reports completion. `main` stays untouched; no automated follow-up or merge is authorized.

| Lane | Exclusive write surface | Bounded deliverable |
| --- | --- | --- |
| [Source managed references](source-managed-references/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**` | Checked `.cg` local object references using frozen Wave 12 runtime graph API |
| [Memory failure assurance](memory-failure-assurance/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Deterministic allocation fault injection and graph/`no_gc` error-path audit |
| [Native memory reports](native-memory-reports/plan.md) | `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Opt-in native report and actual collection-count parity for frozen Wave 12 functions |
| [Pinned LibTorch adapter](pinned-libtorch-adapter/plan.md) | `crates/cgx2-libtorch/**`, `crates/cgx2-bind/**` | Stronger direct native CPU tensor facade contract and offline binding preflight |
| [Portable notebook export](portable-notebook-export/plan.md) | `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, `crates/cgx2-notebook-ui/**` | Reopenable self-contained `.cgnb` export with bounded rich assets |
| [Benchmark task candidates](benchmark-candidates/plan.md) | `bench/**` | Executable candidate tasks, private acceptance fixtures and explicit review/freeze gate |

## Frozen interfaces and ownership

- Before coding, each worker reads `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and its assigned `plan.md`, and follows the AGENTS.md knowledge-graph workflow. Root `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/**`, and `plans/**` are coordinator-owned. Workers do not edit another lane, merge, delegate, message another worker, start follow-on chats, or automate continuation.
- All lanes consume **Wave 12 APIs at dispatch**. The source-memory lane uses existing `Session::alloc_object`, `set_reference` and `reference`; the runtime assurance lane must preserve those signatures and semantics. The native-report lane only targets already supported Wave 12 memory scripts/functions and does not consume new source graph forms. The LibTorch lane stays source-independent; the source-memory lane does not use its new API. Notebook and benchmark work use frozen Wave 12 syntax and execution. No worker needs another worker's uncommitted output.
- The direct LibTorch lane may improve its Rust/C++ native facade and `cgx2-bind` preflight, but no passing test on a mock artifact establishes compatibility with real LibTorch. The intended source-level PyTorch route still requires a later typed tensor/FFI integration wave. It must never load Python or silently use a Python bridge.
- The coordinator checks ownership, memory and foreign boundaries, integrates on `wave13/integration`, and runs combined offline Rust/Python/UI checks. The hard memory release gate needs dynamic checks, source-to-native parity, foreign lifetime review and audit beyond finite seeds. Genuine model traces and a compatible LibTorch artifact remain external evidence gates.

## Wave 12 review facts carried forward

Wave 12 source native C is explicitly unsafe. Integration review found that reused `/proc/self/fd/N` loader names could alias two distinct verified libraries; descriptors now remain held through evaluation and a two-library regression passes. `cgx2 check` now performs static preflight without `dlopen`; runtime identity/version checks occur only on `run`. A benchmark task cannot choose CGx2 as its own baseline. Native memory functions still lack observable collection-count parity, and runtime graph tests lack host allocation fault injection; the first three Wave 13 lanes address these gaps.
