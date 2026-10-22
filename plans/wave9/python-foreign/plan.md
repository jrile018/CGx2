# Declared Python Calls Implementation Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Work directly in this chat. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Provide a validated and allowlisted host-call contract for Python libraries, including a real PyTorch capability check when available, ready for a later `.cg` source adapter.

**Architecture:** Add `cgx2-foreign` as a small typed boundary over `cgx2-python-bridge`. A versioned UTF-8 declaration lists permitted module/attribute call targets, positional/keyword value shapes, return shape, and declared effects/foreign-memory ownership. Validate the declaration before importing or calling Python; preserve Python's actual exception and handle lifetime semantics. No arbitrary source-string evaluation, package installation, or interpreter switching at runtime.

**Tech stack:** Rust 1.98, existing PyO3 bridge, offline JSON crates already in lockfile. CPython environment selected by `PYO3_PYTHON` at build time.

**Spec:** `docs/language-design.md` “Portability, source emission, and foreign libraries”; `crates/cgx2-python-bridge/README.md`; `plans/wave9/README.md`.

## Global constraints

- Own only `crates/cgx2-python-bridge/**`, new `crates/cgx2-foreign/**`, and this plan. Do not edit syntax, sema, CLI, source emitters, root manifests, or docs. This wave does not make `python.call` runnable from `.cg`; the exported Rust API and declaration schema are the handoff to a future source adapter.
- Declaration schema version 1 needs an exact allowlist key, Python module and attribute chain, positional arity and supported `BridgeValue` types, named keyword types, return type, declared `Foreign` effect, and ownership (`copied` or `python_handle`). Reject duplicate keys, unknown fields/types, invalid module/attribute identifiers, unsupported return conversions, and declarations above 64 KiB. No wildcard import/call target.
- The call API validates arity/types/keywords and the explicit allowlist key before crossing CPython. Missing module/attribute, validation failure, Python exception, and unconvertible result stay distinct errors with source-facing stable categories. Python/PyTorch owns handles/tensor storage; CGx2 region budgets and `no_gc` do not cover it. Do not promise a safe arbitrary-library ABI or a GPU backend.
- Keep existing bridge limits: depth 16, 4,096 value nodes, 1,048,576 copied payload bytes, and 128 keywords. Keep `PythonBridge::new`, `call`, `call_kwargs`, and `probe_torch` behavior compatible.

## Review focus

- A declaration typo or blocked call must cause zero Python side effects.
- A wrong return type must report a conversion error after the call, without claiming rollback.
- Python exceptions must retain class/message/traceback but cap output size.
- Clone/drop of returned handles must retain correct Python refcounts.
- Real `torch` absent must be an explicit skip; a fake `torch` import cannot pass capability probing.

## Tasks

### Task 1: Parse a stable declaration contract

**Files:** `crates/cgx2-foreign/{Cargo.toml,src/lib.rs,src/declaration.rs,tests/declaration.rs,README.md}`.

- [ ] Add failing tests for a valid stdlib `math.sqrt` declaration and each malformed/oversized/duplicate/wildcard declaration; assert no import occurs during parsing.
- [ ] Implement `parse_declarations(source: &str) -> Result<Registry, ForeignError>` with exact schema version 1 and deterministic serialization suitable for API tooling. Document the JSON example and ownership/effect labels.
- [ ] Run `cargo test -p cgx2-foreign --offline` for parser tests.

### Task 2: Validated host execution

**Files:** `crates/cgx2-foreign/src/{call,error}.rs`, `tests/call.rs`; minimal compatible bridge changes only if a documented API gap blocks the wrapper.

- [ ] Add failing tests for stdlib scalar/keyword/structured calls, blocked keys, wrong arity/type, missing module, exception, return mismatch, handle lifetime, and argument side-effect ordering.
- [ ] Implement a registry call that accepts the allowlist key, positional `BridgeValue` slice and keyword values, validates first, resolves the declared Python target, then uses bridge `call_kwargs`. Convert only the declared return shape; preserve opaque handle where declared.
- [ ] Run bridge and foreign crate tests with the linked Python library; separately exercise `probe_torch` and report available/unavailable/broken accurately.

### Task 3: Handoff

- [ ] Document how a future `.cg` adapter will map source spans/effects/capabilities to this API, while keeping the current host-only limit explicit.
- [ ] Run fmt and targeted Clippy `-D warnings`, commit only owned paths, and report CPython version/prefix and real PyTorch status.

**Acceptance:** A declaration restricts what the host can call, and validated stdlib calls work through the existing bridge with explicit foreign ownership and stable errors. Source-level Python calls remain a later integration step.
