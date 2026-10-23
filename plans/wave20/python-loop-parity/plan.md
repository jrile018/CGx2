# Python package translation with scalar range-loop parity

**Goal.** Extend Wave 19 readable Python package output to the source language's new allocation-free scalar `for item in range(start, end):` form without pretending arbitrary Python library interop.

**Ownership.** Edit only `crates/cgx2-package/**` and `crates/cgx2-emit-python/**`. Consume the committed Wave 19 syntax/sema/range contract; do not edit those crates, native emitter, CLI, foreign bridge, root Cargo files, or shared docs.

## Deliverable

1. Translate checked scalar range loops in acyclic local packages, including nested loops, lexical shadowing, local pure calls, and zero/negative/`i64`-edge bounds. Endpoints evaluate once. Generated Python must perform CGx2 checked integer operations, not silently use arbitrary-precision Python semantics. Keep effect and capability rejection for unsupported code.
2. Match the source/interpreter/native execution-step budget and local call-depth behavior for this supported package profile. The Python emitter must fail with an explicit CGx2 runtime diagnostic for budget exhaustion or overflow, rather than hanging or returning a different value. If exact parity requires a compiler interface change outside ownership, add a failing focused test and report the boundary.
3. Make emitted module/import names and source-to-Python mapping deterministic. Preserve source-located diagnostics for invalid range arity/type and unsupported memory/tensor/foreign constructs. Do not claim direct PyTorch access: this lane emits Python code and uses Python only as an output target.
4. Compare translated execution with `cgx2 run` and `cgx2c` where available on a small checked corpus. Include successful package entry, cross-module pure call, overflow, budget, and unsupported construct. Python version and subprocess timeout must be recorded.

## Verification and handoff

Run `cargo test --offline -p cgx2-package -p cgx2-emit-python`, focused Python subprocess parity cases, formatter, and owned Clippy `-D warnings`. Commit only owned paths; report SHA, exact parity corpus/results, remaining mismatches, and generated example. Stop for review.
