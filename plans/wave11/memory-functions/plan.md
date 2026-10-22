# Checked memory operations in local functions

> Work in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Let a `.cg` program place the currently supported memory operations inside simple local functions while preserving one checked `cgx2-runtime::Session`, region limits, moves/borrows, and `no_gc` semantics. This is a bounded step toward the unified typed/ownership IR; it is not permission for memory handles to escape or for general typed heap objects.

**Architecture:** Extend the existing parser/checker and CLI interpreter through one shared, typed memory-operation representation or a comparably explicit lowering. Calls in the supported profile execute in the same session and carry source spans. Static ownership facts are scoped to each function; region/owned/managed/borrow values cannot be returned, stored globally, captured, or passed across a function boundary until lifetime rules are defined. Scalar parameter/return values may be used only where the current memory checker can prove a literal/equivalent bounded size; otherwise reject with a precise diagnostic.

**Tech stack:** Rust 1.98; `cgx2-syntax`, `cgx2-sema`, `cgx2-cli`, `cgx2-api-index`, existing `cgx2-runtime::Session`; offline Cargo.

**Spec:** `docs/language-design.md` memory, effects, and compiler sections; `docs/validation-plan.md` hard memory gates.

## Constraints

- Own only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, and `crates/cgx2-api-index/**`. Read but do not edit runtime, emitters, notebook, package, FFI, root manifests/lock, docs or plans. Do not use a new API from the parallel runtime lane.
- Freeze an exact source subset in tests first. A useful target is a local `fn build() -> Unit effects(ManagedAlloc, Collect) caps(MemoryCollect):` that opens a region, allocates/uses/drops a local handle, and closes it. If the existing syntax or type checker needs a smaller compatible form, document and test that form in owned READMEs before implementing. `main` may call that function. Forbid recursion in the memory subset, cross-function handle arguments/returns, dynamic sizes, foreign calls inside the memory function, and hidden region escape.
- Memory operations must use the same static and runtime semantics as the script path. An explicit pure signature cannot call an allocating/collecting memory function. `no_gc` across a local call remains a single guard for all regions; a collect inside it fails before mutation. Manual regions collect only on explicit command. Failed conversion preserves the original owner as the current runtime/CLI contract specifies. Runtime errors keep the `.cg` call/operation span and never emit a success value/report.
- `cgx2 check`, `cgx2 run`, JSON diagnostics, canonical formatting, and API index must agree on the supported profile. Reject unsupported memory code rather than allowing `check` then silently choosing the scalar interpreter. Do not claim native compilation of the new function profile this wave; the parallel compiler lane consumes only the Wave 10 script interface.

## Tasks and checks

1. Add failing fixtures for a valid local memory function, a `main` call using one session, `no_gc` budget failure through a call, manual collection count, owned/managed conversion, and a source-located failed operation. Add negative fixtures for borrow/handle escape, recursion, disallowed arguments/returns, missing effects/caps, and unsupported dynamic size.
2. Implement parser/checker/evaluator wiring with an explicit typed memory operation representation; keep existing scalar functions, scripts, and foreign calls working. Verify no second collector or unchecked pointer path is introduced.
3. Update API index/owned READMEs to distinguish supported function memory from forbidden cross-call handles. Run `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-syntax -p cgx2-sema -p cgx2-cli -p cgx2-api-index --offline`, format and targeted Clippy `-D warnings`, plus `cgx2 check/run` success and failure examples.
4. Commit only owned paths. Report the exact grammar, supported functions, invariant evidence, unsupported cases, tests, and commit SHA.

**Acceptance:** A source memory function runs in the same session as its caller with `no_gc`/manual/ownership guarantees exercised, while every unmodeled escape is rejected at check time.
