# Wave 28: checked package effects and notebook computation

Both lanes start from the same committed Wave 27 review and Wave 28 plans. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger and the assigned plan. Use MegaMemory overview → query → record. Work only in the attached worktree and exclusive paths; verify, commit owned implementation/evidence and stop for user review. No merge, follow-on wave, other-lane edits, root Cargo files or shared design/progress/roadmap/ledger edits.

| Lane | Exclusive write surface | Acceptance milestone |
| --- | --- | --- |
| [Checked package effects](package-effects/plan.md) | `cgx2-sema`, `cgx2-package`, `cgx2-api-index`, `cgx2-cli`, `cgx2-emit-rust`, `cgx2-emit-python`, `cgx2c` | Explicit imported calls receive body-derived transitive effect/capability summaries; pure wrappers work and hidden effects remain rejected |
| [Notebook local accumulators](notebook-local-loops/plan.md) | All five `crates/cgx2-notebook-*` crates | Notebook-defined functions can use local mutable i64 accumulators and literal-bound range loops under conservative cost and replay checks |

## Frozen seams

The package lane may add imported summary metadata and package-aware checking APIs, preserving existing public signatures, `CheckedPackage` accessors, `TypedModule`, `EffectSummary`, scalar/record values and standalone conservative defaults. It must not require notebook changes. The notebook lane uses existing syntax/check/run/value APIs and independently proves its local computation profile; it must not require new package summaries. Imported package functions retain Wave 27 notebook admission, including its existing restriction against mutable/loop bodies. Its import fixtures can use the existing unannotated pure wrappers; the package lane independently tests corrected `effects()` wrappers.

Neither lane edits syntax, ownership/runtime/memory-conformance, verifier, effect-index, security, foreign/binding, benchmark or root Cargo files. Standalone advisory tools without checked package context may continue reporting unknown imported effects; document that boundary. No relaxation of memory/foreign execution or effectful notebook workflows follows from either lane. Report indispensable outside-path changes as concrete blockers.

Use private build/temp directories in each worktree. A shared Cargo target collision invalidated an early coordinator run in Wave 27; final review used a private target. Do not build into the original checkout's shared target.

## Integration acceptance

Review exact commits/ownership, integrate on an isolated branch and keep `main` unchanged. Run combined offline all-feature workspace/UI/benchmark tests, fmt, Clippy, compiler readiness with any coordinator pin refresh, real f32/native Python isolation and memory regression suites. Check imported summary provenance/visibility/capability propagation, truthful public indexes and retained standalone unknown behavior. Check local mutation scope, loop cost before replay, imported helper purity, dependency invalidation and cancellation/failure atomicity. Include a combined notebook calling an explicitly pure multi-module wrapper from a bounded local loop. Update milestones and remaining release gates after review; do not award completion for plans.

## Integrated review

Both deliveries are reviewed on riley/release-completion-plan. See [verification and evidence](REVIEW.md). The next work is the [12-chat finite completion plan](../release-completion/README.md).
