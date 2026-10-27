# Main branch integration — 2026-10-27

`main` consolidates the 93 development branches uploaded to [jrile018/CGx2](https://github.com/jrile018/CGx2). Every uploaded local branch tip is retained as an ancestor. The original branch names remain available; none were deleted or force-pushed.

## Current entry points

- [First-release guide](first-release/README.md): commands, supported profiles, and limitations.
- [Release completion ledger](../plans/release-completion/README.md): current coordination and acceptance gates.
- [S2b-A aggregate ownership plan](../plans/release-completion/s2b-general-ownership/plan.md): unfinished source producer checkpoint.
- [Delivery roadmap](../plans/roadmap.md): historical waves, reviews, and links to their plans.
- [Branch inventory](history/2026-10-27-main-integration.json): exact original branch names, commit IDs, and integration treatment.

## How the histories were combined

The source baseline is the latest reviewed coordinator branch, `riley/release-completion-plan` at `96ca9be98df6b0a037c83877487e1d48ef319dce`.

| Branch treatment | Count | Result |
| --- | ---: | --- |
| Already ancestors of the coordinator baseline | 29 | Retained through the existing history |
| Changes already integrated through equivalent cherry-picks | 63 | Original histories joined with the `ours` merge strategy after `git cherry` confirmed no unique patches; accepted source content retained |
| Unique historical document | 1 | `wave21/integration` merged normally to retain `handoff.md`, now marked as historical |

GitHub's original `main` commit, `b6e4f1d0d6fd11407066d5b2d7e111337c595271`, was merged normally to retain its license and history. Local `main` had previously been uploaded as `riley/local-main`; that branch is also included in the inventory.

Together the original development branches contain 357 reachable commits and 2,533 distinct plan paths. Integration adds history and navigation, without changing compiler, runtime, package, notebook, benchmark, or other implementation files relative to the coordinator baseline.

## Verification and limits

Integration checks verify all 93 original tips are ancestors, every original plan path exists on `main`, implementation files match the reviewed source baseline, new navigation links resolve, formatting passes, and the pushed `main` commit ID matches locally.

This consolidation does not count as new runtime or release acceptance. Existing review evidence retains its original scope and dates. Uncommitted source work, local build outputs, tool state, and internal Codex snapshot refs are not included. No follow-on development wave is dispatched.
