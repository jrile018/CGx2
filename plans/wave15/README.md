# Wave 15: release-gate evidence and execution control

All six chats start from the same reviewed Wave 14 integration commit. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and the assigned `plan.md` before coding. Work only in the attached worktree and exclusive paths, run the listed checks, commit, and stop for the user's review. No worker merges, starts another chat, or automates a follow-up. No worker edits the root Cargo manifest/lock, shared design docs, roadmap, progress table, or coordinator ledger. Consume only frozen Wave 14 APIs.

| Lane | Exclusive write surface | Bounded deliverable |
| --- | --- | --- |
| [Automatic collection assurance](automatic-collection-assurance/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Exercise auto collection safepoints, `no_gc`, faults and graph liveness against a deterministic model |
| [Failure report parity](failure-report-parity/plan.md) | `crates/cgx2-cli/**`, `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Source/interpreter/native memory error class and partial report parity without losing diagnostics |
| [Foreign loader audit](foreign-loader-audit/plan.md) | `crates/cgx2-bind/**`, `crates/cgx2-libtorch/**` | Reject preloaded/substituted closure modules before `dlopen` where observable; bound foreign result allocation and lifetime checks |
| [Notebook execution control](notebook-execution-control/plan.md) | Five `crates/cgx2-notebook-*` crates | Restart, interrupt and stale-output control for retained `.cgnb` sessions |
| [Isolated candidate runner](isolated-candidate-runner/plan.md) | `bench/**` | A fail-closed scored-submission runner with filesystem/network isolation evidence or an explicit unsupported result |
| [Trusted bounded proof](trusted-bounded-proof/plan.md) | `crates/cgx2-verify-core/**`, `crates/cgx2-verify-cli/**` | Solver-backed bounded source-contract result with checked semantics and honest unknown on unavailable solvers |

## Coordinator review gate

Integrate on a new review branch in lane order: automatic collection, failure parity, foreign loader, notebook, proof, benchmark. Review ownership and exact commits; run full offline Rust and Python suites, format, Clippy, notebook UI loopback, source-to-native error fixtures, tensor mock and conditional real-LibTorch tests, and candidate isolation probe. Do not present a mock tensor test as PyTorch conformance or a candidate fixture run as a model benchmark. A passing seeded corpus is evidence, not a universal memory-safety proof. A solver response without a checked encoding is not a trusted proof. When a host feature is unavailable, record the gate as unrun or unsupported. `main` stays unchanged until the user reviews integration.

The release still requires actual memory safety evidence, a compatible real LibTorch run, independently reviewed frozen benchmark tasks and paired trials from two model families, plus remaining language breadth. This wave is bounded to the six interfaces above; it does not claim the first usable release is complete.
