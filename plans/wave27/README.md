# Wave 27: transitive notebook imports and memory range loops

Start both lanes from the same committed Wave 26 review and Wave 27 plans. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger and the assigned plan before coding. Use MegaMemory overview → query → record. Work only in the attached worktree and exclusive paths; verify, commit owned implementation/evidence and stop for user review. Do not merge, start another wave, edit another lane, root Cargo files, shared design/progress/roadmap documents or this ledger.

| Lane | Exclusive write surface | Acceptance milestone |
| --- | --- | --- |
| [Transitive pure notebook imports](notebook-import-closure/plan.md) | All five `crates/cgx2-notebook-*` crates | Checked acyclic package dependencies load transitively with stable nominal identity and complete provenance, while direct notebook imports still control source visibility |
| [Bounded memory range loops](memory-range-loops/plan.md) | `cgx2-ownership`, `cgx2-sema`, `cgx2-cli`, `cgx2-emit-rust`, `cgx2c`, `cgx2-runtime`, `cgx2-memory-conformance` | Existing half-open range syntax executes repeatable scoped memory bodies with a real ownership invariant, cumulative limits and source/native cleanup parity |

## Frozen interfaces and scope

Notebook work consumes committed `CheckedPackage`, module syntax/index/topological order, source hashes/canonical paths, public record metadata, CLI check/run and `Value::Record` interfaces. Its internal import closure does not grant notebook source access to an unimported module. Memory work uses existing `StmtKind::For`, `range(start, end)`, `StmtKind::If` and local memory function syntax. It may extend the owned ownership IR with an explicit loop operation, but must preserve notebook/scalar/public-record APIs and behavior. Neither lane edits syntax, package, Python emitter, verifier, effect-index, security, foreign/binding or benchmark crates. Report indispensable outside-ownership changes as concrete blockers.

The notebook lane remains pure and does not consume memory loops. The memory lane checks existing notebook/Python rejection read-only. Neither feature implies effectful notebooks, arbitrary iterators, general ownership inference, variants/generics or a closed memory-safety release gate.

## Integration acceptance

Review exact commits and path ownership, integrate on an isolated branch and leave `main` unchanged. Run offline all-feature workspace/UI/benchmark tests, formatting, Clippy, public readiness with coordinator compiler-pin refresh if needed, and real f32/native Python isolation. Inspect notebook direct visibility versus internal closure, diamond deduplication, complete transitive provenance and invalidation; inspect memory loop zero-trip/backedge invariants, shared iteration limits, nested call/no_gc analysis, per-iteration cleanup and source/native reports. Run owned instrumentation for new loop cleanup and disclose its coverage. Update concrete milestones and remaining gates after review; plans receive no completion credit.

## Coordinator review before Wave 28

Both worker commits descend from `e3a01dffee423ab93d9cc8cd6d69c0f910c667f4`, stay within their exclusive paths, and had clean tracked worktrees. They are integrated on `riley/wave27-review`; the original checkout and `main` remain unchanged.

| Lane | Worker | Integrated | Result |
| --- | --- | --- | --- |
| Notebook import closure | `d071062` | `6c9d0db` | Deterministic declared dependency closure, direct-source visibility, nominal records and transitive source/canonical-path provenance |
| Memory range loops | `0c1af17` | `af9a0a1` | Explicit ownership loop invariant, scoped iteration values, shared limits and interpreter/native failure parity |

### Coordinator repair

Review found one compatibility regression: notebook `Project::load` rejected valid unnamed single-source packages, even when the notebook needed only a checked project context and no imports. The frozen package loader permits this case. Commit **`9dfa4ad`** restores skipping unnamed entries as import-graph nodes while retaining the checked project snapshot. A regression demonstrates the prior failure, then passes execution, save and reopen after the fix. Named-module graph validation and multi-source package requirements remain intact.

Focused notebook review traced closure deduplication/bounds, direct visibility, canonical path hashes, symbol-producer provenance and worker validation. Independent memory review traced one-iteration outer-state invariance, zero-trip state, fresh loop scopes, shared iteration counts and nested collect/close checks; no confirmed memory defect required repair.

### Combined verification

Final checks used Rust 1.98.1, private `CARGO_TARGET_DIR=/home/john-riley/projects/lang/target/wave28-review`, temporary storage under `target/cgx2-wave21-tmp`, and the existing `LIBRARY_PATH=/tmp/cgx2-wave22-lib` for opted-in bridge linking. An earlier shared-target run failed at doctests because a sema rlib was missing during overlapping default/all-feature builds; it is not counted as passing. The full suite was rerun in the private target. [Verification metadata](evidence/verification.json) records final logs/hashes and the regression's failing/passing runs.

- `cargo test --workspace --all-features --offline`: **840 passed, five ignored**, including **22 HTTP tests**, across 195 test/doc-test groups. Optional external bodies retain their dependency gates.
- `python3 -m unittest discover -s bench/tests`: **122 passed**. Compiler subtree pin refreshed to `92673039d1003fe96f7d2eda933ea3755e7d330e`; root Cargo.lock is unchanged.
- Workspace all-target/all-feature Clippy `-D warnings`, formatting and diff checks pass.
- Fresh private `cargo build --offline -p cgx2c`, then `bench/readiness.py --compiler .../wave28-review/debug/cgx2c`: pinned toolchain and all six public compiler cases pass. The supplied compiler avoids the frozen recipe's shared target; no human/model or scored-isolation gate is claimed.
- Combined `verify_range_consumers.sh` passes notebook/Python memory rejection and scalar verifier unsupported-result checks.
- `verify_range_asan.py`: **24 generated native runs pass** in debug/optimized profiles, including limits, cumulative failures, reserve faults and root cleanup. Runtime/generated code are instrumented with `detect_leaks=0`; system/foreign code is outside the boundary. Leak detection remains host-blocked.
- Fresh default release CLI plus the existing pinned real f32 fixture: native scalar `5000`, explicit CPython child result `9`, real CPU matrix `[[58.0, 64.0], [139.0, 154.0]]`. [Release evidence](evidence/release-python-boundary.json) confirms Python-free native ELF/maps and the explicit Python child boundary.

### Limits and next wave

Notebook import closure is capped at 128 modules, 32 combined functions and 1 MiB lowered/replay source; package inputs remain external. Qualified package calls still receive conservative `Unknown` effects, so explicitly pure wrappers can be rejected. Memory loops use restricted literal/parameter/index endpoints, a shared 100,000 iteration cap and 1,000,000 statement-step cap; outer ownership mutation, loop-local graphs/regions and general iterators remain outside the profile. See [notebook evidence](../../crates/cgx2-notebook-kernel/WAVE27-EVIDENCE.md) and [memory evidence](../../crates/cgx2-memory-conformance/release-gate-wave27.md).

Progress now records **18 reviewed bounded deliveries across Waves 22–27**. [Wave 28](../wave28/README.md) targets checked cross-module effect/capability summaries and notebook-local integer accumulators with statically bounded range loops. Broader types/libraries, effectful notebook workflows, leak assurance, live proof, scored isolation and paired trials remain open.
