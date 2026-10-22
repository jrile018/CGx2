# CGx2 Python Value and Keyword Bridge Plan

> Implement this plan in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Make the existing dynamic CPython bridge useful for ordinary libraries that accept keyword arguments and structured values while keeping conversion, aliasing, and foreign memory behavior explicit.

**Architecture:** Extend `PythonBridge` with `call_kwargs(callable, positional, keyword)` using PyO3 argument vectors/dictionaries, not string evaluation. Add bounded bridge values for `bytes`, lists/tuples, and string-keyed dictionaries or equivalent explicit conversion methods. Preserve opaque `PyHandle` fallback for arbitrary objects and cycles. Validate limits before recursive conversion; keep Python exceptions and the linked-interpreter identity visible.

**Tech stack:** Rust 1.98, existing PyO3 bridge and stdlib Python for mandatory tests; genuine PyTorch optional only when installed in the Python interpreter selected at build time.

**Specification:** `crates/cgx2-python-bridge/README.md`, `docs/language-design.md` interop boundary, `plans/wave7/README.md`.

## Owned paths and boundaries

- Own only `crates/cgx2-python-bridge/**` and this plan. Do not change parser, source checker, CLI, emitters, notebook, root manifests/lock, or other lanes. Preserve `import_module`, `getattr`, `call`, `to_value`, tensor APIs, and existing error meanings.
- Add a clear `call_kwargs` signature with deterministic keyword ordering, duplicate-key rejection, and bounded keyword count. Python call failures return their class/message/traceback; invalid bridge input returns a typed conversion error before the call. No `eval`, `exec`, shell, or auto-install.
- Copy scalar/bytes/container data crossing into a CGx2-owned Rust value. Handles continue to alias Python objects and release references on drop; do not claim zero-copy ownership transfer or that CGx2 `no_gc` controls Python/PyTorch. Limit nested container depth, total elements, and byte payloads; detect cycles and return an opaque handle or an explicit limit error rather than recursing forever. Document which path is used and keep Python integer overflow behavior explicit.
- Mandatory integration tests use real stdlib modules (`json`, `collections`, or similar) with positional and keyword calls, Unicode, bytes, nested containers, missing attributes, Python exceptions, and handle reference lifetime. Optional PyTorch tests use the actual installed package and report a skip when absent; no fake `torch` module may be counted as real validation.
- This is a host bridge API, not `.cg` source syntax. It permits calling any importable Python object dynamically if the selected interpreter has it, but type safety is restricted to validated bridge values and the tensor common API.

## Tasks

### Conversion and call API

- [ ] Add red tests for keyword forwarding, duplicate/invalid keyword names, nested round trips, byte copies, recursion/cycle limits, and overflow.
- [ ] Implement value conversion with explicit budgets and opaque fallback. Keep error paths free of leaked Python references.
- [ ] Implement `call_kwargs`; have `call` delegate if that preserves behavior without breaking existing callers.

### Foreign memory and documentation

- [ ] Test that cloned handles alias and remain valid after the original is dropped, while copied structured values remain independent. Confirm Python exceptions stay distinguishable from conversion errors.
- [ ] Update crate README with exact API signatures, build-time interpreter selection, copy-vs-handle table, foreign allocator limits, and two real stdlib examples.

### Verification and handoff

- [ ] Run `cargo test -p cgx2-python-bridge --offline -- --nocapture` with the matching Python development library configured, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-python-bridge --all-targets --offline -- -D warnings`.
- [ ] Commit only owned paths; report the interpreter and whether real PyTorch ran or skipped.

**Acceptance:** Python stdlib keyword/structured calls work through bounded, explicit conversions, with opaque fallback and honest foreign-memory ownership.
