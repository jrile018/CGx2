# Wave 24: conditional control flow and execution cost

All three coding chats start from the same committed reviewed base. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and your assigned plan before coding. Use MegaMemory overview → query → record. Work in the attached worktree, respect exclusive paths, commit one bounded deliverable, and stop for user review. Do not merge, begin another wave, edit another lane, the root Cargo manifest/lockfile, or shared design/progress/roadmap documents.

| Lane | Exclusive write surface | Bounded result |
| --- | --- | --- |
| [Conditional statements](conditional-statements/plan.md) | Syntax, sema, CLI, compiler, Rust/Python emitters, effect-index, security, package, verify-cli, and all five notebook crates | Mandatory-else Boolean statements across supported scalar execution paths, with every AST consumer handled explicitly |
| [Verified loader cost](verified-loader-cost/plan.md) | `crates/cgx2-bind/**`, `crates/cgx2-libtorch/**` | Measured, behavior-preserving cost reduction with substitution/content checks retained |
| [Native and Python worker baselines](native-worker-baselines/plan.md) | `bench/**` | Reproducible optimized scalar-native and explicit Python-child observations on the frozen base |

## Frozen inputs and boundaries

The benchmark lane measures this common base, including Wave 23's Python-free native process, without consuming concurrent conditionals or loader work. The loader lane keeps public binding/LibTorch APIs and lock schemas stable; it may build the base CLI for its own measurements but may not edit it. The conditional lane consumes the existing frozen loader/foreign APIs and must preserve Python isolation.

The AST change is deliberately owned by one lane. Splitting parser, sema, emitters, and their exhaustive consumers among concurrent chats would require unpublished interfaces and overlapping edits. Other lanes do not modify any AST consumer. The coordinator owns any eventual compiler pilot-pin refresh and root lockfile change.

No new memory corpus lane is started merely to increase test counts: Wave 23's expanded pressure model passed, while leak-enabled instrumentation requires a compatible host. This wave does not claim that memory release, trusted proof, LLM/human trials, or broader libraries are finished. Tracked implementation remains 92% until a remaining area gate actually closes.

## Integration review gate

After user review/completion, inspect each exact commit and path ownership; integrate on an isolated review branch. Run combined offline Rust/all-feature/UI/benchmark tests, formatting, Clippy, public reference readiness with updated pins if needed, conditionals through interpreter/native/Rust/Python/package/notebook paths, memory regressions, and real pinned f32 with native ELF/maps evidence. Verify branch-local scope, returns, checked errors, dependency/effect union, unsupported memory/proof behavior, loader mutation rejection, and benchmark build/profile/sample provenance. Keep `main` unchanged for user review.
