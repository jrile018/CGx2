# Wave 10: native semantics and foreign boundary

Five user-owned chats start from one verified `wave10/integration` commit. Each completes one bounded deliverable, commits its exclusive files, and stops for review. The estimate includes roughly 20–25% coordinator review and integration time. The compiler source lane is wide because parser, checker, and evaluator share one contract; the other lanes consume existing Wave 9 interfaces or add independent crates.

| Lane | Exclusive write surface | Reviewable result |
| --- | --- | --- |
| [Explicit Python source calls](source-python/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-foreign/**`, `crates/cgx2-api-index/**` | One typed `.cg` call into an explicitly declared Python-only library, with source spans, effects, ownership and errors |
| [Native memory and contract execution](native-memory/plan.md) | `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Native memory-script slice plus contract-preserving native scalar emission; unsupported forms diagnosed |
| [Memory failure-path hardening](memory-failure/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Allocation/collection failure invariants exercised and repaired without changing public signatures |
| [Direct native library boundary](native-ffi/plan.md) | new `crates/cgx2-ffi-native/**`, new `crates/cgx2-libtorch/**` | Checked C ABI fixture and version-pinned LibTorch adapter prototype with no Python dependency |
| [Python target parity](python-emission/plan.md) | `crates/cgx2-emit-python/**` | New source contracts/effects preserved or rejected explicitly in Python output |

## Frozen interface and ownership rules

- All five start from the same commit. Only the source Python lane changes source grammar, checker, CLI evaluator, foreign host, and API index. Native and Python emitters read the Wave 9 AST and checker as they exist at the shared base; they cannot use new source-Python syntax this wave. New foreign declarations are a source-lane contract and are not yet native C ABI calls.
- Only the memory lane changes `cgx2-runtime::Session`; it preserves public signatures. Native emission uses the existing runtime crate without relying on a new API from that lane. The Python emitter handles the existing scalar contract AST, not the later source-Python AST changes.
- The native FFI lane owns new crates only. It may use published native C/LibTorch interfaces but cannot edit `Cargo.toml`, `Cargo.lock`, shared docs, or another lane's crate. A conditional LibTorch test on a host without LibTorch is reported unavailable, never passed. The generic C fixture must run on this host.
- Root `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/**`, `plans/**`, benchmark files, and this ledger belong to the coordinator. `crates/*` includes the new crates automatically. Workers may read those files and put implementation notes in their owned crate READMEs. No worker merges branches, messages another worker, creates a follow-on chat, or automates later work.
- Before coding, read `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and the assigned plan; follow project knowledge-graph workflow. Run the plan's checks, commit only owned paths, and report the commit, verification, and limitations to the user.

## Coordinator acceptance after the chats finish

Review each commit against its plan and exclusive paths, integrate on an isolated review branch, run the combined offline Rust/Python suites, CLI and native differential fixtures, C ABI fixture, UI HTTP tests with loopback permission, format and Clippy. Keep `main` untouched. Reconcile source-Python checks with emitted target diagnostics before crediting a language-level interop claim. Direct `.cg` LibTorch calls, full native managed-memory codegen, trusted SMT proof, persistent notebook effects, GPU auto execution, all-ten-ecosystem coverage, and real LLM superiority remain open until their own integration and evidence gates pass.
