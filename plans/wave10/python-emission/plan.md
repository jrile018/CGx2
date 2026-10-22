# Python source output contract parity

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Ensure Python source emission preserves Wave 9 scalar `requires`/`ensures` behavior and does not silently drop effect/capability declarations or unsupported semantics.

**Architecture:** Keep the existing Python target as a bounded translation of checked scalar `.cg`. Add an emission validation pass for the existing AST declaration fields, then generate checked pre/postconditions with equivalent evaluation order and result binding for supported pure scalar contracts. Reject unsupported calls, effects, ownership or memory forms with source spans before generating Python.

**Tech stack:** Rust 1.98, `cgx2-emit-python`, Python 3 runtime for differential fixtures; offline Cargo.

**Spec:** `docs/language-design.md` effects and source-emission sections; `docs/validation-plan.md` portability suite.

## Scope and contract

- Own only `crates/cgx2-emit-python/**`. Read but do not edit source AST/checker/CLI, Python bridge, root manifest/lockfile, docs, plans, or native emitter. Use Wave 9 AST and checker from the shared base, not syntax introduced by another Wave 10 lane.
- Match `cgx2 run` for supported pure scalar `requires` and `ensures`: precondition before body, postcondition after return, `result` in postcondition, no success output on failure. Preserve checked `i64` behavior and Boolean comparisons. If a construct cannot be matched, emission fails with a stable code/span; no silently missing contract.
- Explicit effect/capability declarations cannot be interpreted as grants for operations Python output performs. Reject foreign/memory, unknown transitive effects, `no_gc`, or any target-only behavior not modeled by this emitter. Record the emitted profile clearly; this task does not implement direct LibTorch or Python-only host access.
- Compare both success and failure class/order with the CLI; generated Python error text need not be byte-for-byte identical if a documented structured mapping is supplied. Existing scalar fixtures stay stable.

## Tasks

1. Add failing differential tests for true/false pre/postconditions, `result`, checked arithmetic in contracts, an undeclared effect, and unsupported target construct. Include a case where old emission would produce a successful output after a failing contract.
2. Implement contract checks or diagnostic rejection, metadata gate, and source-located errors. Update owned README with exact supported profile and emitted Python behavior.
3. Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-emit-python --offline`, fmt, targeted Clippy `-D warnings`, and Python execution of generated fixtures. Commit only owned paths; report the commit, checks and unsupported cases.

**Acceptance:** Python emission either preserves each accepted scalar contract/effect rule or reports a diagnostic; it never emits plausible Python that bypasses a required check.
