# Interpreter and native failure report parity

Use the attached worktree; read shared Wave 15 references and this plan. Commit only owned paths and stop for review.

**Goal.** Wave 14 compares native and interpreter memory reports on success, but interpreter failures expose only diagnostics. Provide a bounded source-level failure record so a failed `.cg` graph program can compare diagnostic class, source span, and post-failure memory state across interpreter and native debug/release outputs.

**Ownership.** Only `crates/cgx2-cli/**`, `crates/cgx2-emit-rust/**`, and `crates/cgx2c/**`. Read the frozen Wave 14 runtime API; do not change it, memory-conformance, foreign crates, notebooks, root Cargo files, docs or plans. Preserve existing CLI success output and JSON schema compatibility, extending fields additively if needed.

## Deliverable

1. Add an opt-in structured failure result/report in interpreter and generated native programs. Include error code, file/span, region and total counters at the point of failure, collection count, and whether rollback left prior roots/edges intact. Do not print a success report for an error or hide a primary diagnostic behind report generation.
2. Compare equivalent checked `.cg` source fixtures for out-of-range slot, stale/released handle, cross-region edge, manual limit, `no_gc` budget, and host allocation fault. Run interpreter and native debug/release where representable. Assert actual outcomes, not only a handwritten expected constant. Preserve native package rejection for unsupported graph code.
3. Make injected-failure parity runnable through a documented `cgx2c` command or a clearly named test-only option, rather than mutating generated Rust by hand. Test that the option cannot leak into ordinary release builds.
4. Update owned README with command, JSON fields, partial-report semantics, and explicit unsupported cases. Keep scalar, Python-only, and direct native tensor routes working through the frozen baseline.

## Verification

Run offline tests for the three owned crates, format and targeted Clippy with warnings denied, plus direct interpreter/native debug/release failure fixtures. Report parity table, exact command lines, any runtime API dependency and commit SHA.

**Acceptance.** Supported failure cases have source-located diagnostic and comparable partial memory report on interpreter/native paths, with no silent success or target mismatch.
