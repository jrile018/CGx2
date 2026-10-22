# Wave 14: source-to-runtime closure

Start all six chats from the reviewed `wave14/integration` commit. Each chat owns only the paths below, commits one bounded result in its attached worktree, runs its specified checks, and stops for the user's review. No worker merges, starts another chat, or automates follow-up. Read `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and the assigned `plan.md` before coding. The frozen baseline is Wave 13 plus the coordinator's integration repairs; do not consume another Wave 14 lane's uncommitted API.

| Lane | Exclusive write surface | Bounded result |
| --- | --- | --- |
| [Memory release audit](memory-release-audit/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Reproducible reference-graph allocation, collection, and fault-path assurance with explicit release-gate findings |
| [Native source graph](native-source-graph/plan.md) | `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Compile the frozen checked local `.cg` managed-reference subset and compare native reports with interpreted reports |
| [Source tensor call](source-tensor-call/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**` | A narrow, explicit unsafe `.cg` CPU float64 tensor call path to the frozen direct LibTorch adapter, with no Python middle layer |
| [LibTorch closure](libtorch-closure/plan.md) | `crates/cgx2-libtorch/**`, `crates/cgx2-bind/**` | Pin and verify the local facade's transitive LibTorch dependency closure and reject loader ambiguity |
| [Notebook output cells](notebook-output-cells/plan.md) | `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, `crates/cgx2-notebook-ui/**` | Persist and selectively rerun captured cell output in the `.cgnb` workflow without replaying unchanged cells |
| [Candidate review packet](candidate-review-packet/plan.md) | `bench/**` | Reviewable task candidates and a workspace-bound hidden acceptance/evidence packet; no invented model results |

## Integration order and gates

Review every worker commit and its test evidence. Integrate on a new review branch in this order: runtime, native graph, LibTorch closure, source tensor, notebook, benchmark. Resolve shared `Cargo.lock` changes only in the coordinator branch; workers may not edit the root workspace manifest, shared design docs, roadmap, progress table, or this ledger. Run offline combined Rust and Python suites, format, Clippy, direct `.cg`/native/notebook smoke, and private candidate acceptance against an actual candidate workspace. Loopback UI checks require socket permission. Real LibTorch, sanitizer, solver, GPU, and paired model runs must be reported as unrun when unavailable.

The hard memory gate remains open until source, interpreter, native output, stress/fault tests, and documented foreign lifetimes agree. A mock facade does not prove a real PyTorch route. Candidate tasks are not benchmark wins until independently reviewed, frozen, and run with preregistered paired model trials. The coordinator does not merge `main`; the user reviews the integration result first.
