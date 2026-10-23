# Checked source float32 CPU matrix call

**Goal.** Expose Wave 21's pinned direct float32 rank-two LibTorch route from a bounded `.cg` source package without Python or an implicit dtype switch.

**Ownership.** Edit only `crates/cgx2-sema/**` and `crates/cgx2-cli/**`, including owned tests and fixtures. Read but do not edit syntax, binding, LibTorch, package, `cgx2c`, notebook, root Cargo files, or shared docs. Document an exact missing adapter API if this contract cannot be executed without another lane's files.

## Deliverable and contract

1. Reuse the existing foreign native declaration and quoted matrix literal grammar. Admit one exact source symbol `cgx2_torch_matmul_f32` with `unsafe`, `ownership copied`, `(String, String) -> String`, `effects(Foreign)`, `caps(native)`, and `cgx2.libtorch.cpu@VERSION`. Bind only to schema-2 `tensor-f32-cpu-matrix-v3` and unchanged facade ABI `cgx2-libtorch-facade-v1`. Keep the float64 `cgx2_torch_matmul` source profile behavior intact.
2. Check row-major rank-two input, dimensions of 1..=32 and at most 1,024 elements, compatible inner dimensions, and finite values. Parse the existing decimal literal as `f64`, narrow to `f32` using IEEE round-to-nearest ties-to-even, accept finite inexact rounding, and reject overflow to infinity or a nonzero value narrowed to zero. Test subnormal handling and a value such as `0.1` that rounds inexactly. Reject wrong layout/version and mixed dtype before foreign loading with source spans and stable error codes. Do not add AST variants, general tensor types, GPU, or a Python fallback.
3. Dispatch through the committed `Torch::from_f32_matrix`, `MatrixF32::matmul`, facade-reported shape/dtype, checked output copy, and explicit release. Validate output shape and finiteness; preserve first operation failure while reporting cleanup failures separately. Keep float32 and float64 Rust handles distinct.
4. Test `.cg` check/run against a mock v3 facade and a copied pinned real Torch 2.11 CPU package when present: 2x3 by 3x2 reference within `1e-5`, dimension mismatch, nonfinite/overflow input, wrong layout/version, missing export, wrong reported dtype, and repeated release. Confirm no mapped `libpython` in the real route. A skipped real run is not a pass.

## Verification and handoff

Run offline sema/CLI tests and existing float64 source matrix regressions, a real pinned float32 source test if available, `cargo fmt --all --check`, and owned Clippy `-D warnings`. Commit only owned paths; report SHA, fixture, exact commands/results, real versus mock status, tolerance, and unsupported source/GPU profiles. Stop for review.
