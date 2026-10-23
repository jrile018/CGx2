# Wave 25: public record APIs and conditional verification

Two capability lanes start from one reviewed plan commit. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and the assigned plan before coding. Use MegaMemory overview → query → record. Work only in the attached worktree and exclusive paths, run the specified checks, commit one bounded deliverable, and stop for user review. Do not merge, start another wave, or edit shared design/progress/roadmap/ledger files or root Cargo manifest/lockfile.

| Lane | Exclusive write surface | Acceptance milestone |
| --- | --- | --- |
| [Public record package APIs](public-record-apis/plan.md) | Syntax, sema, package, CLI, cgx2c, Rust/Python emitters, effect-index, security, five notebook crates | Explicitly public immutable scalar records retain module-qualified nominal identity through supported same-package calls and both emitters |
| [Conditional contract verification](conditional-contracts/plan.md) | `crates/cgx2-verify-core/**`, `crates/cgx2-verify-cli/**` | Return-only conditional trees have selected-branch finite evaluation, independent source audit and sound conditional SMT definedness |

## Independence and scope

The verifier lane uses Wave 24's committed mandatory-else `StmtKind::If`, scalar expressions and function contract fields. Those interfaces are frozen. The record lane may add record visibility metadata and imported-record environments but must not change `Function`, `StmtKind::If`, scalar `ExprKind` or existing public scalar checking behavior. Neither verifier crate constructs `Record` struct literals or depends on record fields at this base. An unavoidable incompatible public interface change is a blocker to report, not an invitation to edit the other lane.

The record lane owns package and notebook consumers so richer package acceptance cannot silently enable an unsupported notebook profile. Imported records in notebooks remain explicitly rejected with regression coverage; full retained imported-record execution is a later bounded task. No transitive record reexports, generics, nested records, foreign layouts, source memory joins or general standard library is added here.

No benchmark-only or repeated memory-corpus lane is scheduled. Existing suites remain regression gates. Wave 24's measurements are preserved; mandatory loader checks stay intact. Live solver availability, leak-enabled instrumentation, scored isolation and human/model trials remain external evidence gaps. The historical 92% estimate is replaced by [milestones and release gates](../progress.md), not a new invented percentage.

## Coordinator acceptance

Review exact commits and ownership, integrate on an isolated branch, and test combined offline Rust/all-feature/UI/benchmark suites, formatting, Clippy and public readiness with any coordinator-owned compiler pin refresh. Exercise two modules with same-named records and both emitters, privacy/invalid import errors, old scalar/notebook behavior, branch-selected overflow/counterexamples, independent finite-source audit and adversarial solver transcripts. Preserve existing real f32/native Python isolation and memory regressions. No release or solver-proof claim follows merely from finite tests. Keep `main` unchanged for user review.
