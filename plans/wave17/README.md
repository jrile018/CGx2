# Wave 17: root observability and usable source breadth

All six coding chats start from the same reviewed Wave 16 integration commit on `wave17/integration`. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and your assigned `plan.md` before coding. Work only in the attached worktree and exclusive paths below. Run the specified checks, commit, and stop for the user's review. Do not merge, automate, create follow-on chats, edit another lane, or edit the root Cargo manifest/lock or shared design/roadmap/progress documents. Report unsupported host capabilities and evidence as unrun.

| Lane | Exclusive write surface | Bounded deliverable |
| --- | --- | --- |
| [Runtime root census](runtime-root-census/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Allocation-free root registration census with cleanup/fault conformance |
| [Reusable memory functions](reusable-memory-functions/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-api-index/**`, `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Bounded scalar parameters in memory functions across checker, interpreter and native output |
| [Reproducible LibTorch evidence](reproducible-libtorch/plan.md) | `crates/cgx2-bind/**`, `crates/cgx2-libtorch/**` | Reviewable pinned real CPU LibTorch package recipe and negative ABI/lifetime fixtures |
| [Pure notebook functions](pure-notebook-functions/plan.md) | The five `crates/cgx2-notebook-*` crates | Persistent pure user-function cells with selective rerun and effect rejection |
| [Proof encoding audit](proof-encoding-audit/plan.md) | `crates/cgx2-verify-core/**`, `crates/cgx2-verify-cli/**` | Exhaustive small-domain cross-check of the supported SMT fragment and precise solver trust status |
| [LLM pilot readiness](llm-pilot-readiness/plan.md) | `bench/**` | Frozen syntax-comparison/task-equivalence packet and actual-token trace validation, without fabricated model results |

## Wave 16 review baseline

Six commits were integrated: 1,024-seed direct-runtime memory corpus and cleanup evidence; operation and final-unwind source/native failure snapshots; stronger mapped-module and tensor-handle checks; explicit notebook effect rejection and stale outputs; aggregate cgroup containment; and bounded solver observation taxonomy. Coordinator review fixed a benchmark process that could close stdout/stderr to escape normal wall-time monitoring. It also clarified that final-unwind zeros come from source bindings rather than a runtime root census, and that a worker-reported LibTorch 2.14 CPU run used temporary pins that were not committed for independent reproduction.

Combined offline Rust workspace tests excluding socket UI, UI loopback tests, formatting, Clippy with warnings denied, and 83 Python benchmark tests pass. On this host the isolation probe returns `unsupported_isolation:cgroup_controllers_not_delegated:cpu,memory,pids`; Z3/CVC5 are absent. A real LibTorch run was reported by a worker but is not yet reproducible from committed pins. The memory correctness, trusted proof, independently reviewed benchmark tasks and paired LLM trials remain open release gates.

## Coordinator gate

Review exact commits and ownership before integrating, run combined tests and source-to-native fixtures, and fix proven defects on the integration branch. A direct-runtime test does not establish whole-language memory safety; a solver response without checked obligations is not a global proof; a local tensor test without archived pins is not portable compatibility evidence; a benchmark packet without independent review/model trials is not an LLM result. `main` remains unchanged pending the user's review.
