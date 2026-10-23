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

## Coordinator review before Wave 27

Both worker commits descend from `a574cea65d801c284e86c057bbab3fdd7ebb4549` and stay inside their exclusive paths. Worker tracked files were clean. Integration is on `riley/wave26-review` in the attached coordinator worktree; the original checkout and `main` remain unchanged.

| Lane | Worker commit | Integrated commit | Reviewed result |
| --- | --- | --- | --- |
| Notebook public records | `5a0720b` | `cd33daf` | Public scalar record imports preserve defining-module identity through checked worker transport, replay, provenance and lifecycle routes |
| Conditional memory | `1d14943` | `86fb6d3` | Both source checkers and interpreter/native paths support bounded guards, conservative joins, per-arm byte/token scopes and failure cleanup |

Focused review traced notebook identity conversion, checked result types, imported annotations/dependencies, canonical package identity and selective invalidation. A separate memory review traced both checker paths, nested local-call/collect/close discovery, ownership joins, fresh arm scopes and selected-path cleanup. No confirmed defect required a coordinator code repair.

### Combined verification

Rust 1.98.1, `CARGO_TARGET_DIR=/home/john-riley/projects/lang/target`, `TMPDIR=/home/john-riley/projects/lang/target/cgx2-wave21-tmp`, and `LIBRARY_PATH=/tmp/cgx2-wave22-lib` for opted-in bridge linking. [Verification metadata](evidence/verification.json) records exact log paths and hashes.

- `cargo test --workspace --all-features --offline`: **808 passed, five ignored**, including **21 HTTP tests**, across 192 test/doc-test groups. Optional external test bodies retain their dependency gates.
- `python3 -m unittest discover -s bench/tests`: **122 passed**. Initial discovery at `bench` found zero tests and was corrected. The first actual suite detected the expected stale compiler pin; refreshing `compiler_source_git_tree` to `e26d2cab5da44b44d0ae2f5efe8bb959f4ad10da` resolved it. Root Cargo.lock is unchanged.
- Workspace all-target/all-feature offline Clippy with `-D warnings`, `cargo fmt --all --check`, and `git diff --check` pass.
- `python3 bench/readiness.py --build`: pinned toolchain and all six public reference cases pass. Independent human review, genuine paired trials and scored cgroup isolation remain unrun/unsupported.
- Fresh default release CLI build plus `verify_python_boundary.py`: native scalar `5000`, explicit Python child result `9`, real Torch 2.11 CPU f32 `[[58.0, 64.0], [139.0, 154.0]]`. [Release evidence](evidence/release-python-boundary.json) confirms Python-free native ELF/maps; the explicit Python route launches its selected CPython child.
- `sh crates/cgx2c/scripts/verify_conditional_consumers.sh`: combined notebook and Python memory rejection passes.
- `python3 crates/cgx2c/scripts/verify_conditional_asan.py`: **22 generated native runs passed** across debug and optimized profiles, including success, budget/region/fault failures and final root cleanup. `ASAN_OPTIONS=detect_leaks=0`; runtime and generated code are instrumented, not system/foreign libraries or compiler processes. Leak detection remains blocked by the documented host ptrace limitation.

### Capability boundaries and next wave

Notebook imports still require direct declarations of dependency modules. Records remain immutable, scalar-field-only, pure and local-package based; saved/exported notebooks still need their external package inputs. The conditional memory guard accepts Boolean literals or exact two-i64 comparisons; branch-local regions/graphs and source memory loops remain unsupported. See [notebook handoff](../../crates/cgx2-notebook-kernel/WAVE26-EVIDENCE.md) and [memory handoff](../../crates/cgx2-memory-conformance/release-gate-wave26.md).

The progress ledger now records **16 reviewed bounded deliveries across Waves 22–26**. They do not close the broad release gates. [Wave 27](../wave27/README.md) connects transitive pure package dependencies to notebook execution and bounded range loops to memory checking/native execution.
