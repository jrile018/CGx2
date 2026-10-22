# CGx2 Checked Memory Script Execution Plan

> Implement this plan in your attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Make the Wave 4 source-checked memory script profile execute through the real `cgx2-runtime::Session`, with no hidden collection inside `no_gc` and clear runtime failures.

**Architecture:** Keep the existing scalar evaluator unchanged for scalar modules. Add a separate memory-script evaluator that parses once, invokes `cgx2_sema::check_module` and ownership checking before any side effect, then interprets only the exact reserved memory intrinsics. Store runtime values as non-`Clone` owned buffers, region IDs, managed handles, and borrow tokens. Use recursive block execution with `Session::no_gc(budget_bytes)` guards; the guard borrows the session and enforces budget on all regions. Root each live managed handle and unroot it on explicit drop, so a collection cannot invalidate a source-live value. Add a CLI memory report mode that renders `Session::memory_report()` after successful execution.

**Tech stack:** Rust 1.98, existing `cgx2-syntax`, `cgx2-sema`, `cgx2-ownership`, `cgx2-runtime`; offline only, no `unsafe`.

**Specification:** `docs/language-design.md` memory sections, `docs/validation-plan.md` memory correctness gates, `plans/wave5/README.md`.

## Ownership and fixed contracts

- Own only `crates/cgx2-cli/**` and this plan. Do not edit parser, sema, ownership checker, runtime, root manifests/lock, or another lane's files. Add a dependency on the existing runtime in this crate's manifest as needed.
- The supported source profile is exactly the one described in `crates/cgx2-sema/README.md`. `memory.region(mode: auto|manual, limit: N)` uses the runtime's auto trigger of 1 MiB initially; expose that choice in CLI docs. `memory.owned_bytes(N)` allocates owned bytes; `memory.move` transfers the value without clone; `memory.alloc_bytes(region,N)` creates and roots a managed handle; `memory.borrow`/`borrow_mut` create tokens governed by the static checker; `end_borrow` ends them. `use` validates a live object, `drop` releases it/unroots a handle, `collect` and `close` call runtime APIs. No implicit cross-region move, promotion, or foreign memory.
- `run_source` is a public low-level API. It must not bypass static memory checks. A checked memory script must use the new evaluator, while scalar scripts retain Wave 4 behavior. Unsupported memory syntax/operations must fail before mutations. Do not serialize handles into `RunResult::Value` or fabricate a scalar tail value; a memory script may return `Unit`.
- Add `cgx2 memory report FILE.cg` (or a documented equivalent) using the same checker/evaluator and versioned runtime report. A failure exits nonzero with source-located diagnostic and does not print a success report. Report never claims Python/PyTorch allocations are CGx2 memory.

## Review focus

1. A manual region never collects except on `memory.collect`; an auto region collects only at its documented allocation safepoint. Collection in nested `no_gc` returns a checker diagnostic; budget exhaustion returns a defined runtime error with no collection.
2. Managed source bindings are rooted while live. Explicit `drop` unroots; collecting afterward invalidates the handle. Closing a region with live roots fails through the static or dynamic gate. Across two regions, a collection never invalidates the other region's handle.
3. Owned move leaves exactly one value; use after move and conflicting borrows fail before execution. Runtime values are not cloned to fake ownership.
4. `Session::memory_report()` after execution shows actual allocation/collection/failure counters. Check JSON parses and text is stable; no report is printed after failure.
5. Existing scalar CLI and benchmark pilot still pass. Low-level `run_source` and user-facing `cgx2 run FILE.cg` agree on memory safety boundaries.

## Tasks

### 1. End-to-end memory evaluator

- [x] Add red tests for a valid manual region script, owned move, auto region, and two-region isolation using public `run_source` and CLI. Include a scalar regression fixture.
- [x] Implement non-`Clone` runtime binding state and direct AST evaluation for only the checked intrinsic forms. Recheck static validity before opening any region. Use source spans for runtime errors and stable codes.
- [x] Exercise explicit drop/close, rooted liveness across collection, stale handle rejection, and cleanup on error. An error may leave earlier mutations inside the discarded session, matching the language design; it must not leak a success result.

### 2. `no_gc` and reporting

- [x] Add nested guard tests with allocation below/above budget, across two regions, and an explicit collect attempt. Assert exact zero collection delta inside guards, including failures.
- [x] Implement recursive guarded block execution. Allocation errors must not trigger collection or silently increase budgets. Do not interpret Python/foreign allocation as protected by `no_gc`.
- [x] Add the memory report command and integration fixture. Verify `schema_version`, counters, and no output after a rejected script.

### 3. Verification and handoff

- [x] Run `cargo test --workspace --exclude cgx2-python-bridge --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy --workspace --exclude cgx2-python-bridge --all-targets --offline -- -D warnings`, and `python3 bench/pilot.py parity --cgx2 target/debug/cgx2` after building the current binary.
- [x] Document exact source operations, the 1 MiB auto trigger, runtime errors, report command, and still unsupported memory features. Commit only owned paths.

**Acceptance:** At least one checked `.cg` program creates/manual-collects/closes a region through the actual runtime, and nested `no_gc` tests show zero collection under success and failure. This remains a bounded memory profile, not a whole-language memory-safety proof.
