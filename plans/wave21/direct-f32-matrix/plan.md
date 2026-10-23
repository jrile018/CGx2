# Additive direct float32 CPU matrix route

**Goal.** Extend the direct PyTorch-compatible LibTorch C++ route to the common float32 rank-2 dtype without loading CPython or changing the frozen float64 ABI.

**Ownership.** Edit only `crates/cgx2-bind/**` and `crates/cgx2-libtorch/**`. Do not edit source compiler, Python bridge, notebook, benchmark, root Cargo files, or shared docs.

## Deliverable

1. Specify an additive versioned layout and C facade symbol set for owned row-major CPU float32 rank-2 handles: create from copied `f32` input, inspect shape and dtype, matrix multiply compatible shapes, copy into caller buffer with checked capacity, and release exactly once. Keep all v1/v2 float64 symbols, status codes, ownership rules, and schema-2 lock validation unchanged. Do not reinterpret a float32 buffer as float64 or expose a raw borrowed pointer beyond the call.
2. Add Rust safe wrappers with distinct float32 handle types so float32 and float64 cannot be mixed accidentally. Check zero/overflow/over-limit dimensions, nonfinite input/output policy, incompatible inner dimensions, too-small copy capacity, wrong version/symbol, and every failure cleanup path. Limit elements according to the existing bounded copy policy; report actual shape and dtype independently of the caller's expectation.
3. Build or prepare a copied pinned CPU Torch package using the real native facade if local dependencies permit. Record exact Torch version, facade and dependency hashes, architecture, linker flags, binding lock, and absence of mapped `libpython`. Run a known float32 result and repeated create/multiply/copy/release cycles; compare to a separately computed reference within a declared tolerance. Mock C tests must be labeled mock and cannot substitute for the real test.
4. Run an available dynamic tool over the mock and, if technically possible, real facade boundary. Document what was actually instrumented. Source `.cg` syntax and GPU are outside this lane; preserve the Wave 20 `.cg` float64 matrix behavior.

## Verification and handoff

Run offline bind/libtorch tests, pinned preflight and real CPU test when local package can be prepared, formatter, and owned Clippy `-D warnings`. Commit only owned paths; report SHA, ABI/layout names, real versus mock evidence, numeric tolerance, and remaining lifetime risks. Stop for review.
