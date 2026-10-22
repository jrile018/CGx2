# CGx2 Memory Source Frontend Implementation Plan

> Implement this plan in your attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Make memory control and ownership operations readable in `.cg`, canonical under `cgx2 fmt`, and statically checked through the existing ownership operation IR. Preserve the scalar preview and reject memory execution that is not implemented.

**Architecture:** Extend the syntax AST/parser/formatter with named call arguments and a structured `no_gc(budget: N):` statement. Recognize a deliberately small set of memory intrinsics and owned-value moves/borrows in semantic checking, lower their source spans into `cgx2_ownership::Op`, then run `check_ops`. The frontend is the single owner of all AST-consuming crates this wave, so the CLI and Python emitter remain compilable and explicitly reject unsupported execution/emission. Keep the lowering deterministic and conservative.

**Tech stack:** Rust 1.98, workspace crates, offline dependencies only; no `unsafe` or hidden Python/runtime calls.

**Specification:** `docs/language-design.md` sections “Values and regions” and “Choosing when collection runs”; `plans/wave4/README.md`.

## Owned paths and fixed boundaries

- Own `crates/cgx2-syntax/**`, `cgx2-sema/**`, `cgx2-ownership/**`, `cgx2-cli/**`, `cgx2-emit-python/**`, and `cgx2-api-index/**`. Do not edit runtime, notebook, Python bridge, benchmark, root manifests/lock, or shared docs.
- Keep `parse_module`, `check_module`, CLI `check/run/fmt`, Python scalar emission, and public API indexing compatible for all previously accepted scalar files. New public AST variants may be added; update every exhaustive match in owned crates.
- Preserve the Wave 3 public ownership `Op` and diagnostic codes `CGO001`–`CGO005`. Add only a source-lowering diagnostic family if needed; keep stable `code`, `message`, UTF-8 byte `span`.
- A function or script that mixes unsupported effects, dynamic calls, control flow, or foreign aliases with owned values must receive an explicit unsupported diagnostic. Never omit an operation and then call the program safe.

## Initial syntax contract

Parse and format these forms with source spans; settle exact AST names in code and document them in the crate README:

```cg
let scratch = memory.region(mode: manual, limit: 4096)
let buffer = memory.owned_bytes(128)
let moved = memory.move(buffer)
no_gc(budget: 256):
    let item = memory.alloc_bytes(scratch, 64)
memory.collect(scratch)
```

- `mode` accepts `auto` or `manual` as explicit enum-like names, never an arbitrary runtime variable in this slice. `limit` and `budget` accept nonnegative integer literal bytes with checked conversion; suffix units can follow later. Reject duplicate, unknown, or missing named arguments with a source span. Ordinary positional calls remain unchanged.
- `no_gc` is a structured statement with a block, not a regular function call. Nested blocks are legal. Lower entry/exit to `EnterNoGc`/`ExitNoGc`; `memory.collect` inside must report `CGO004`.
- The sample above is a *syntax/checking* sample. `cgx2 run` must explicitly report unsupported memory execution until a later runtime integration. `cgx2 check` may accept only the precise operations the lowering models; unsupported forms must fail. Do not silently coerce generic user functions named `memory.*` into checked intrinsics unless the namespace is reserved and documented.

## Review focus

1. A valid scalar program produces the same AST behavior, CLI output, and Python output as Wave 3.
2. Formatter round trips source memory forms, is idempotent, and preserves UTF-8 byte spans in diagnostics.
3. Move then use, borrow conflict, live handle at region close, and collect inside nested `no_gc` fail through ownership checks with source locations.
4. Budget and region limits reject overflow, negative values, duplicate keywords, and unsupported dynamic values instead of panicking or accepting them.
5. `cgx2 run` and Python source emitter refuse memory code with explicit diagnostics. The API index remains stable for ordinary public signatures.

## Tasks

### 1. Syntax and canonical formatting

- [ ] Add red parser/formatter tests for named arguments, `no_gc` nesting, malformed indentation, duplicate labels, mixed positional/named calls, and UTF-8 spans. State the chosen mixing rule in the README.
- [ ] Extend lexer/parser/AST and formatter. Keep ordinary call precedence and scalar formatting stable.
- [ ] Run `cargo test -p cgx2-syntax --offline` and show idempotence on both memory and scalar fixtures.

### 2. Conservative type and ownership lowering

- [ ] Add red semantic/source-check tests for the listed memory intrinsics, owned moves and use after move, region close with a live handle, conflicting borrows, collection under a guard, and unknown operations.
- [ ] Implement source lowering to `Vec<Op>` with exact spans. If the existing IR cannot represent a safe source form, extend the IR in your owned crate or diagnose unsupported; never approximate a safe result. Keep any public lowering API documented.
- [ ] Run `check_ops` after lowering. Separate syntax/type diagnostics from ownership diagnostics in a stable way that the CLI can render as JSON Lines.
- [ ] A region handle and managed handle must be distinguished from `i64`; typed signatures for intrinsics must reject wrong arguments. Do not claim memory budgets are statically guaranteed beyond literal checks; runtime enforcement remains required.

### 3. Consumer compatibility and safety gates

- [ ] Update CLI `check`, `fmt`, and `run` for new AST variants. `check` reports all safe-subset diagnostics; `run` rejects all memory constructs until execution is actually connected. Ensure `run_source` cannot bypass this with an unchecked memory path.
- [ ] Update the Python emitter to return an unsupported-target diagnostic for memory forms; update the API index to reject or represent new public signatures deterministically.
- [ ] Add cross-crate tests for valid scalar replay, invalid memory source, rejected runtime execution, and rejected Python emission.
- [ ] Run `cargo test --workspace --exclude cgx2-python-bridge --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy --workspace --exclude cgx2-python-bridge --all-targets --offline -- -D warnings`. If a dependency/tool issue blocks a gate, record the exact command and output. Commit only owned paths.

**Acceptance:** The new memory source forms parse, format, and receive conservative static diagnostics; no user-visible path silently executes or emits them with altered semantics. Runtime memory execution remains an explicit next-wave integration gate.
