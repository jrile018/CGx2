# Wave 26: notebook records and conditional memory

Two capability lanes start from the same committed reviewed base. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger and the assigned plan. Use MegaMemory overview → query → record. Work in the attached worktree and exclusive paths, verify the bounded result, commit and stop for user review. No merge, follow-on wave, other-lane edits, root Cargo manifest/lockfile or shared design/progress/roadmap/ledger changes.

| Lane | Exclusive write surface | Acceptance milestone |
| --- | --- | --- |
| [Imported notebook records](notebook-public-records/plan.md) | All five `crates/cgx2-notebook-*` crates | Pure imported public scalar records retain defining-module identity through cells, worker transport, stale-state handling and persistence |
| [Conditional source memory](conditional-memory/plan.md) | `cgx2-ownership`, `cgx2-sema`, `cgx2-cli`, `cgx2-emit-rust`, `cgx2c`, `cgx2-runtime`, `cgx2-memory-conformance` | Bounded existing-guard conditional memory executes only the selected path with conservative ownership joins, cleanup and source/native parity |

## Frozen interfaces

Notebook work consumes Wave 25's public-record `CheckedPackage`/import metadata, existing `Value::Record`, and scalar/record CLI/check APIs. It does not depend on concurrent memory support. Memory work consumes the committed `StmtKind::If` and existing `cgx2_ownership::Op::If`; source comparisons already parse to internal comparison calls. It must preserve all public scalar/record/notebook-facing signatures and behavior. No lane edits syntax, package, Python emitter, effect-index, security, verifier, foreign or benchmark crates. If a new shared public accessor or an outside-ownership edit is truly required, report the exact blocker rather than privately changing a frozen contract.

The memory profile stays separate from pure notebooks and Python emission. Existing notebook admission and Python memory rejection must continue to work. Source/native memory conditionals do not imply general ownership inference, branch-local region creation, object-graph declaration joins, or new syntax. Notebook records do not imply effectful or foreign cell execution.

## Integration acceptance

Review exact commits/ownership and integrate on an isolated branch. Run combined offline all-feature Rust/UI/benchmark tests, fmt, Clippy, readiness with any coordinator compiler-pin refresh, notebook record lifecycle/worker checks, memory branch/failure/source-native parity, and existing real f32/native Python isolation. Inspect nested local-call/no_gc analysis, selected-branch allocation accounting and cleanup, imported record identity/provenance, and unsupported-target rejection. Update concrete milestones and remaining gates; keep `main` unchanged. A finite corpus is not a memory safety proof, and host-blocked leak/live-solver/LLM gates remain explicit.
