# Checked `.cg` CPU matrix multiplication

**Goal.** Make the Wave 19 real 2D float64 LibTorch operation callable from a readable `.cg` source program using the existing pinned C++ facade directly. Python must not be loaded or used as a middleman.

**Ownership.** Edit only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, and `crates/cgx2-cli/**`. Consume, but do not edit, the frozen v2 facade in `cgx2-libtorch` and schema-2 metadata from `cgx2-bind`. Do not edit `cgx2c`, runtime, package/Python translation, notebook, or root Cargo files.

## Deliverable

1. Extend the existing lexical tensor source profile with `unsafe foreign native pub fn matmul_f64_cpu(left: String, right: String) -> String effects(Foreign) caps(native) = torch_cpu.cgx2_torch_matmul runtime "cgx2.libtorch.cpu@VERSION" ownership copied`. The one local `main` calls this declaration with quoted nested row-major literals, for example `matmul_f64_cpu("[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]", "[[7.0, 8.0], [9.0, 10.0], [11.0, 12.0]]")`. Accept rectangular nonempty rank-2 float64 matrices with 1..=32 rows/columns and at most 1,024 elements per input and result; each value must be finite. Output is a nested row-major string such as `[[58.0, 64.0], [139.0, 154.0]]`. Preserve source-located diagnostics. Keep tensor handles opaque; no general tensor value or implicit memory management claim.
2. `cgx2 check` must reject missing or mismatched schema-2 binding, wrong `libtorch` route/layout/ABI, bad shape, nonfinite or excessive literals, output capacity mismatch, and unsupported package/source combinations before a foreign call. The diagnostic must identify the source span where practical.
3. `cgx2 run` must load the existing pinned package, create two input handles, call facade v2 matrix multiplication, copy the checked result, and release every handle on success and each failure branch. Never fall back to Python, a mock, or a separate handwritten numeric implementation when the real package is missing. Preserve the first operational error and surface cleanup errors separately where API permits.
4. Test the 2x3 by 3x2 known result `[58, 64, 139, 154]`, zero/bad dimensions, one mismatched shape, wrong binding metadata, missing package, and repeated calls. Include a conditional real-package test with exact package pin/preflight; mark it skipped when absent. State explicitly that `cgx2c` native compilation of this tensor profile and generic Python-library access are outside this lane.

## Verification and handoff

Run `cargo test --offline -p cgx2-sema -p cgx2-cli`, focused real-package test when the pinned copy exists, formatter check, and owned-crate Clippy with `-D warnings`. Commit only owned paths; report source example, exact commands/results, package pin, SHA, and unsupported profiles. Stop for review.
