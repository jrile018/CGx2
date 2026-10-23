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

## Reviewed integration — 2026-10-23

The three deliveries descend from `425cb0a5467d98b2cf9565be19067b2f39f23fff` and change only assigned paths. Tracked worker files are clean; incidental untracked `.serena/` metadata in worker checkouts was left untouched. Integration is isolated on `riley/wave24-review` in the attached coordinator worktree. The original checkout and `main` were not changed.

| Lane | Worker | Integrated | Result |
| --- | --- | --- | --- |
| Conditional statements | `fda1cf7` | `611e182` | Mandatory-else Boolean statements, branch scopes/returns, supported interpreter/native/emitter/package/notebook consumers |
| Verified loader cost | `9102028` | `a1962f7` | Opt-in timing and a measured blocker; no speedup claimed or validation removed |
| Native/worker baselines | `f593997` | `b73b78b` | Frozen-base release scalar AOT and explicit Python-child paired observations |

### Independent checks

Rust 1.98.1; shared build cache `/home/john-riley/projects/lang/target`; `TMPDIR` is its `cgx2-wave21-tmp` subdirectory. Opted-in bridge tests use the existing `LIBRARY_PATH=/tmp/cgx2-wave22-lib` linker-name workaround. Exact log hashes are in [verification.json](evidence/verification.json).

- `cargo test --workspace --all-features --offline`: **753 passed, five ignored**, including **20 HTTP tests**. Optional external probes without required dependencies retain their own skip behavior.
- `python3 -m unittest discover -s bench/tests`: **122 passed** after coordinator maintenance. The initial attempt retained in the logs had ten read-only-worktree temporary-directory errors and one expected stale public compiler subtree pin. Rerunning with worktree write permission and refreshing `compiler_source_git_tree` to `9288b55f7d4ff0ae173ebfd522f67d0985165888` resolved them; root Cargo.lock is unchanged.
- Workspace all-feature/all-target Clippy `-D warnings`, formatting and diff checks pass.
- `python3 bench/readiness.py --build`: pinned toolchain and all six public reference checks pass. Human review/model trials and scored cgroup isolation remain unrun/unsupported.
- Fresh default release build plus the committed Python-boundary verifier passed native scalar output, explicit isolated Python output, and **completed real pinned Torch 2.11 f32 execution** returning `[[58.0, 64.0], [139.0, 154.0]]`. Exact launcher/helper hashes and ELF/maps are in [release evidence](evidence/release-python-boundary.json). Native routes have no observed Python dependency/mapping/child; the explicit Python route has the selected CPython child. This coordinator run does not claim a new debug/unified-feature real-matrix census.

### Interpretation

Conditional statements are a new supported language capability, with mandatory else, Boolean guards, branch-local scope, explicit definite returns, and Unit fallthrough. Conditional memory programs and proof bodies still reject; notebooks admit pure function bodies and retain their existing top-level/effect restrictions. See the [consumer evidence](../../crates/cgx2-cli/CONDITIONALS-EVIDENCE.md).

The [loader report](../../crates/cgx2-bind/evidence/wave24/REPORT.md) directly attributes about 97% of the tiny-matrix run to 15.01 GB of repeated full-content hashing. Buffer experiments did not demonstrate a useful safe speedup. Instrumentation and same-inode mutation regressions were delivered; the performance bottleneck remains open.

The [benchmark report](../../bench/WAVE24-NATIVE-WORKER-RESULTS.md) retains both measured series and setup failures. Thirty execution pairs per case show selected scalar AOT paired runtime ratios near 1.05 and Python-child whole-route ratios about 4.5–4.7 versus direct CPython. The latter includes additional parsing/checking/transport and does not score a PyO3 tiny-call target. Submillisecond native whole-process timings are not compute-only throughput. No whole-language, inference or LLM target is established.

### Progress reporting correction

The historical 92% estimate gave broad areas near-full credit while assigning several large remaining features to their final points. That granularity hid real incremental work and made the remaining scope appear too small. It is retired as the current headline, not increased to an unsupported number. [Progress](../progress.md) now reports reviewed milestones and explicit open gates. All **three Wave 24 bounded deliveries are reviewed**; this does not mean all release gates passed. Wave 25's two capability plans are in [its ledger](../wave25/README.md).
