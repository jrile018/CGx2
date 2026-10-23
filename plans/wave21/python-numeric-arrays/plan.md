# Checked numeric arrays for Python-only libraries

**Goal.** Improve the explicit CPython route for libraries that have no suitable native ABI, without using Python as a middleman for the direct LibTorch route.

**Ownership.** Edit only `crates/cgx2-python-bridge/**` and `crates/cgx2-foreign/**`. Do not edit source compiler, LibTorch adapter, root Cargo files, notebook, or shared docs.

## Deliverable

1. Add one declared value shape `f64_array_1d_copy_v1` for 0..=1,048,576 finite `f64` elements in contiguous one-dimensional storage. Use a shape-specific checked element and byte budget: validate `len.checked_mul(8)` before allocating or reading, cap each input and output at 8,388,608 payload bytes, and cap aggregate live copied payload within one call at 16,777,216 bytes. Keep the existing generic 4,096-element/1 MiB limits for scalar/list/dict values unchanged. CGx2-owned input is copied into a Python `array('d')` or equivalent owned buffer; Python output must expose a contiguous one-dimensional native-endian float64 buffer and is validated then copied back into CGx2-owned storage. A Python list is not silently treated as the declared buffer shape. No pointer, borrowed slice, or Python object may outlive its owning interpreter/GIL scope. Reject wrong rank, dtype, byte order, noncontiguous view, excessive length, and nonfinite values with stable error classes.
2. Route only allowlisted declared Python-only functions through this value shape. Preserve existing scalar/list/dict and exception behavior. A function that returns an incompatible object must fail without exposing partially initialized output or leaking a reference; repeated calls and thrown Python exceptions need lifetime regression tests. Avoid claiming zero-copy or control of Python's allocator/GC.
3. Test against the real embedded CPython `array('d')` from the standard library and an optional installed NumPy fixture, recording exact interpreter/package versions. A missing NumPy installation yields a clearly skipped optional test; standard-library conversion tests must still run. Do not use PyTorch in this path or change the direct LibTorch route.
4. Document copied bytes, ownership, GIL boundary, maximum length, and a microbenchmark for call overhead versus the existing scalar/list route. Keep performance results scoped to the measured host and call shape.

## Verification and handoff

Run offline bridge/foreign tests, repeated exception/lifetime tests, formatter, and owned Clippy `-D warnings`; record required `LIBRARY_PATH` or interpreter pin. Commit only owned paths; report SHA, supported declaration, exact commands/results, and remaining Python-library compatibility limits. Stop for review.
