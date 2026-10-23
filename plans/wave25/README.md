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

## Reviewed integration — 2026-10-23

Both deliveries descend from `7f4c4525c61da165a4b628f34eb095521a14b349` and stay inside assigned paths. Tracked worker files are clean; incidental untracked `.serena/` metadata was left untouched. The attached coordinator worktree integrates on `riley/wave25-review`; the original checkout and `main` remain unchanged.

| Lane | Worker | Integrated | Result |
| --- | --- | --- | --- |
| Public record APIs | `c5ba133` | `4165673` | Public/private visibility and module-qualified nominal types through package CLI, native, Rust and Python execution |
| Conditional verification | `18db690` | `2336e23` | Return-only decision trees, selected-branch evaluation/SMT definedness and independent source audit |

Read-only review traced visibility/signature collection, pure package lowering, target name encoding and notebook rejection. No confirmed actionable defect required a coordinator code repair. The verifier retains full static admission of both branches and checks only the selected path for runtime overflow. The independent source oracle rejects deliberately corrupted lowering and SMT branches/definedness.

### Independent verification

Rust 1.98.1, shared target `/home/john-riley/projects/lang/target`, temporary storage under its `cgx2-wave21-tmp`, and the existing `LIBRARY_PATH=/tmp/cgx2-wave22-lib` workaround for opted-in bridge linking. [Verification metadata](evidence/verification.json) records exact log locations and hashes.

- Full offline all-feature workspace: **778 passed, five ignored**, including **20 HTTP tests**. Optional external bodies retain their existing dependency gates; the total is not a claim that every external route ran.
- All Python benchmark regressions: **122 passed**. The coordinator refreshed `compiler_source_git_tree` to `394a00299ca6ed99ae1d67471a4e360d43468d02`; root Cargo.lock is unchanged.
- Workspace all-target/all-feature Clippy with `-D warnings`, formatting and diff checks pass.
- Public syntax readiness with `--build`: pinned toolchain and all six reference tasks pass. Human/model trials and scored cgroup isolation remain unrun/unsupported.
- A fresh default release CLI passed the Python-boundary verifier: native scalar output `5000`, isolated Python result `9`, and completed real Torch 2.11 CPU f32 result `[[58.0, 64.0], [139.0, 154.0]]`. [Exact release ELF/maps evidence](evidence/release-python-boundary.json) shows no libpython dependency or observed native mapping; only the explicit Python route has the selected CPython child. No new debug/unified-feature real-path census is claimed.
- Integrated verifier smoke: `conditional.cg --domain x:-1:1` passes all six finite assignments; `conditional-inactive-overflow.cg` at i64 MAX passes its one finite assignment; `conditional-overflow.cg` returns the validated `flag=false, x=MAX` overflow counterexample. Successful finite verdicts mean `finite_domain_only`; no genuine solver proof was performed.

### Capability and limits

Public immutable scalar records now cross explicit local package module APIs without losing defining-module identity or privacy. Same-shaped records from different modules remain distinct; target collisions/Unicode/`self` fields and atomic output failures are covered. Notebook import admission deliberately rejects public imported records in this wave. See [record evidence](../../crates/cgx2-package/docs/wave25-public-record-apis.md).

The verifier accepts complete return-only conditional trees up to **32 nested conditionals** and **1,024 source nodes**. Both branches are statically admissible; selected-branch overflow and counterexample replay are independently checked. Calls, local state, loops, records and memory remain outside this verifier fragment. Genuine Z3/CVC5 proof remains unavailable; mocks are transport/rejection tests. See [verifier evidence](../../crates/cgx2-verify-cli/wave25-evidence.md).

Both bounded deliveries are reviewed. The [progress ledger](../progress.md) now records **14 deliveries across Waves 22–25**, without reviving the retired percentage or claiming release gates closed. [Wave 26](../wave26/README.md) targets imported notebook records and bounded source/native conditional memory.
