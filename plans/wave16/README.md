# Wave 16: close measured release gaps

All six lanes start at the reviewed Wave 15 integration commit on `wave16/integration`. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and the assigned `plan.md` before coding. Work in the attached worktree and only in the listed exclusive paths. Run the specified checks, commit, and stop for the user's review. Do not merge, launch follow-on chats, alter the root Cargo manifest/lock or shared design documents, or count an unavailable external tool as a pass.

| Lane | Exclusive write surface | Bounded result |
| --- | --- | --- |
| [Memory release evidence](memory-release-evidence/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Reproducible auto/manual/`no_gc` state-machine and dynamic-tool evidence, with precise unrun gates |
| [Failure lifecycle parity](failure-lifecycle-parity/plan.md) | `crates/cgx2-cli/**`, `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Compare failure-time roots and post-cleanup state for checked source programs across interpreter and native builds |
| [Foreign lifetime and closure](foreign-lifetime-closure/plan.md) | `crates/cgx2-bind/**`, `crates/cgx2-libtorch/**` | Bound loader identity and tensor lifetime claims with adversarial and compatible real-LibTorch evidence where available |
| [Notebook effect boundary](notebook-effect-boundary/plan.md) | The five `crates/cgx2-notebook-*` crates | Deterministic stale-output behavior for admitted effectful cells and interrupt/restart continuation |
| [Benchmark containment](benchmark-containment/plan.md) | `bench/**` | Fail-closed aggregate-resource isolation and independently reviewable task packet, without fabricated model trials |
| [Trusted bounded proof](trusted-bounded-proof/plan.md) | `crates/cgx2-verify-core/**`, `crates/cgx2-verify-cli/**` | Carry the unfinished Wave 15 solver lane to a reviewable bounded proof result or explicit `unknown` |

## Wave 15 review baseline

Five of six Wave 15 worker commits were integrated. The trusted bounded-proof lane had no commit and receives no progress credit. Coordinator review additionally fixed loader inspection of executable maps lacking `.so` names, notebook stdin-pipe timeout and slow-client interrupt delays, pinned Rust compiler hashing in benchmark scoring, and narrowed memory failure-report wording. Offline workspace tests, Clippy, formatting, UI loopback tests, and 77 benchmark tests passed on the repaired snapshot. A real compatible LibTorch artifact, live Z3/CVC5 proof, host namespace isolation, independently reviewed task set, and paired LLM trials remain unavailable or unrun.

## Integration rule

The coordinator reviews each commit, tests the combined result, and fixes integration defects on this review branch. Workers do not merge. A passing unit test does not satisfy the release memory, proof, notebook, foreign, or benchmark gates by itself. Report exact commands, fixtures, skips, and commit IDs.
