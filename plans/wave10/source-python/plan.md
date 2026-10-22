# Explicit Python-only calls from `.cg`

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Make an ordinary `.cg` script call a declared Python-only library through the existing `cgx2-foreign` host, with checked signatures, explicit effects/capabilities, and source-located failures. This route must be visibly separate from the direct LibTorch PyTorch route.

**Architecture:** Add the smallest readable foreign declaration syntax that binds a qualified CGx2 name to a pinned Python module/function and a scalar signature. Resolve it in the existing checker and execute through `cgx2-foreign`; do not add a second evaluator or silently import Python modules by name. A checked source program should call a stdlib fixture such as `math.isqrt`, and an invalid module/signature/return should fail deterministically.

**Tech stack:** Rust 1.98, `cgx2-syntax`, `cgx2-sema`, `cgx2-cli`, `cgx2-foreign`, `cgx2-api-index`, PyO3; offline Cargo and the installed CPython runtime.

**Spec:** `docs/language-design.md` interoperability and effects sections; `docs/interoperability-design.md` foreign contract v1; `docs/validation-plan.md` foreign boundary and CLI gates.

## Scope and contract

- Own only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-foreign/**`, and `crates/cgx2-api-index/**`. Do not edit root manifests, lockfile, shared docs, plans, Python bridge, or an emitter.
- Freeze and test a concise source declaration before implementing. It must name the `python` route, target module/callable, argument and result types, ownership, `Foreign` effect and `ForeignCall` capability. Support fixed-width `i64`, `Bool`, and `String` values initially; reject unsupported types and mutable/borrowed foreign buffers. Require a local allowlist/declaration so arbitrary imports do not run from an untrusted string. A package/version pin or explicit local-runtime identity is recorded; do not claim reproducibility from an unpinned package.
- Preserve Wave 9 contracts: a `requires` failure stops before the foreign call, `ensures` runs only after successful typed return, missing `Foreign`/`ForeignCall` fails static checking, and a Python exception becomes a structured CGx2 error with source span. Explicitly reject a `torch` binding on this Python-only route, since PyTorch is reserved for the direct LibTorch route. Do not claim this wave makes `.cg` call LibTorch.
- Keep source diagnostics as stable code, message, zero-based half-open UTF-8 span, and JSON. The API index must display target route, signature, ownership, effects, capability, and runtime requirement. No exception unwinds across FFI unchecked.
- Since emitters start from the same base, they must reject this new construct after integration until a later native/output adapter exists; do not edit their crates. If current emitter validation could silently discard it, add a source flag/AST distinction that permits a coordinator integration guard and disclose the gap.

## Tasks

1. Add failing parser/formatter and semantic tests for valid declaration, duplicate/unknown route, wrong type/count, missing effect or capability, and Unicode spans; implement the minimal syntax and static gate.
2. Add failing CLI tests for a deterministic Python stdlib call, runtime missing module/callable, Python exception, wrong return type, contract ordering, and JSON source spans; connect checked calls to the existing host. Test rejection of `torch` on this route.
3. Extend the API index and crate READMEs with a runnable `.cg` example and exact unsupported cases. Run `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-syntax -p cgx2-sema -p cgx2-cli -p cgx2-foreign -p cgx2-api-index --offline` after arranging the host's versioned Python library for linking if needed, plus fmt and targeted Clippy `-D warnings`.
4. Commit only owned paths and report the source example, negative fixture, commit, checks, and remaining native/output gap.

**Acceptance:** The same `.cg` syntax is readable and statically checked; a declared Python-only call executes through the declared host route with correct result/error and no implicit PyTorch fallback.
