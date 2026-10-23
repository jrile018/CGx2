# Direct CPU inference v4

`Torch::open_pinned` accepts a schema-3 `tensor-f32-cpu-inference-v4` package.
Existing v1 vector/v2 matrix/v3 float32 signatures and lock files remain valid.
New API/version/export layouts are in `facade/facade.h` and
`cgx2-bind/SDK-LOCK-SCHEMA3.md`. CPU, contiguous row-major rank-2 float32 only;
nonempty dimensions and at most 1,048,576 elements per matrix (4 MiB copy).
`from_f32_matrix` copies finite input; `shape`, `info` (dtype code 1), `device`
and bounded copy check the retained expected shape. LibTorch owns tensor
storage; host buffers can be dropped after creation.

`MatrixF32::add` requires exact equal shape and the same adapter, without
broadcasting. `relu` preserves shape. Both publish distinct owned handles and
catch C++ exceptions; add checks finite output and ReLU checks finite input.
The new C++ operations use `c10::InferenceMode`. Creation/matmul are the
preserved v3 operations on tensors with no gradient state. `matmul → add →
relu → matmul → add` is the pinned two-layer inference sequence. All calls
retain the DSO/closure, check identity/inodes/hashes, and borrow input handles
for the call. Explicit release and Drop share the existing consume-on-attempt
rule; returned alias/error cleanup retains the v3 lifetime audit. No autograd,
GPU, training, serialized model import, zero-copy or general tensor rank claim.

Build with the existing PREPARE.md commands, then add `--inference` to
`prepare_pinned.py` instead of `--matrix-f32`. It generates exact schema-3 locks
and copied non-system dependencies. No download/install occurs. Then:

```sh
cargo run --offline -p cgx2-bind --bin cgx2-bind -- check "$PACKAGE"
LD_LIBRARY_PATH="$PACKAGE" CGX2_LIBTORCH_REAL_PACKAGE="$PACKAGE" \
  cargo test --release --offline -p cgx2-libtorch --test inference \
  real_pinned_two_layer_inference_no_python -- --ignored --exact --nocapture
LD_LIBRARY_PATH="$PACKAGE" cargo run --release --offline -p cgx2-libtorch \
  --example infer_cpu -- "$PACKAGE"
```

The acceptance computes a separate scalar f32 CPU reference (absolute tolerance
1e-5), checks dtype/device/shape, drops source buffers, repeats create/infer/copy/
release, rejects duplicate release and checks process maps contain no libpython.
These are correctness/lifetime observations, not leak proof or throughput/p95
evidence. This is a Rust facade example; new `.cg` inference syntax/types and
ownership/effect lowering remain S2/S3 work. Existing source f32 matmul remains
on its reviewed v3 profile. Real performance comparisons require a quiet host
and both pinned native LibTorch and explicit Python baselines, with all hashing,
startup/copy/release costs charged. No benchmark ran for this deliverable.
