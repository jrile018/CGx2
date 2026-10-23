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
