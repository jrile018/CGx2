# CPU and GPU environment readiness

Status: **partial**. Rust, native OCaml, coherent Python/PyTorch CPU, standalone native LibTorch CPU, and a real discrete AMD Vulkan compute dispatch pass. PyTorch/LibTorch GPU integration remains blocked. CGx2 SDK/package integration is **NOT_PROVEN**. These are readiness smoke tests, with no timing or performance conclusions.

## Frozen inputs and scope

Consumed `../input-manifest.json`, baseline `4ed17189867b11e2a3c8280f02a05e3017f3ae5a`, branch `riley/lean-architecture-review-execution`. The receipt records the input manifest SHA256. Writes are confined to this environment evidence lane and `/tmp/cgx2-arch-review-environment`. No crate, workspace manifest/lock, shared Python/opam environment, driver, device permission, original WIP or other evidence lane was changed. No commits or pushes were made.

## Executed checks

| Path | Version/device | Observed result |
|---|---|---|
| Rust native | rustc1.98.1, x86_64 CPU | Standalone source compiles and buffer/failure fixture passes |
| OCaml native | ocamlopt5.5.1 from existing `serena-ocaml` switch | Standalone source compiles and alias/buffer/failure fixture passes |
| Python CPU | Isolated CPython3.12.15 | Buffer view, values, sum, rejected/accepted budget fixture passes |
| Python ML CPU | Torch2.11.0+cpu; int32 and float32 | Alias/failure fixture and 2x2 float32 matrix multiply pass |
| Native LibTorch CPU | Same wheel's ATen headers/libtorch_cpu/c10; C++11 ABI enabled | C++ native program compiles, links, executes fixture; transitive ELF dependencies resolve; no libpython mapped after execution |
| Discrete GPU compute | RX9070XT PCI1002:7550, RADV GFX1201, Vulkan, Mesa26.2.4; wgpu0.31.1 | Genuine WGSL dispatch of four i32 and f32 elements, explicit synchronization/readback, exact expected output |
| Python ML GPU | Torch2.11.0+cpu; HIP/CUDA versions null | Actual GPU tensor allocation fails; CPU fallback was not used |
| OpenCL | Loader present, no ICD | `clGetPlatformIDs` returns -1001, zero platforms |

All CPU buffer fixtures start with `[1,-2,3,4]`, reject a logical16-byte request against budget15 without changing the buffer, admit budget16, transform `x*2+1`, and read `[3,-3,7,9]` through a view/alias with sum16. Rust uses a shared slice after the accepted mutation; Python, OCaml and Torch retain an alias across the update. The small wrappers deliberately use the same local policy. They do not establish language memory guarantees, heap/RSS bounds, cross-language zero-copy interop, CGx2 semantics or end-to-end foreign-call safety. The native ML source exercises the same ATen int32 fixture. The Python float32 inference result is `[[2,4],[6,8]]`.

The GPU probe selects **only** vendor0x1002, device0x7550, `DiscreteGPU`, `Vulkan`; it rejects llvmpipe/CPU fallback and does not select the iGPU. It dispatches WGSL i32 affine transformation and f32 multiply-by-two, shape `[4]`, workgroup size4, then waits with `queue.on_submitted_work_done_sync()` and performs blocking `read_buffer`. Exact integer output `[3,-3,7,9]` and float output `[2,4,6,8]` pass. Inputs are copied from host and results copied back. This toy compute test does not validate a tensor inference library, numerical behavior of larger kernels, GPU lifetime/failure policy, or an existing CGx2 GPU binding. No GPU failure-atomicity claim is made.

## Hardware, permissions and backend limitations

Actual host: Linux7.0.0-42-generic x86_64, Ryzen7 9800X3D,16 logical CPUs. Both AMD render nodes and `/dev/kfd` open successfully with the existing render-group permission. The sandbox hides these nodes; actual GPU checks were run through authorized elevated execution. No device/security changes were needed. `raw/sandbox-inventory.json` and `raw/inventory.log` preserve that distinction.

Vulkan also enumerates the Raphael integrated GPU PCI1002:13c0 and a separate llvmpipe CPU adapter. Hardware existence alone did not count as success; the selected discrete adapter executed the shader. Mesa warns that RADV is non-conformant/testing-use-only. Preserve this driver caveat and pin before interpreting production readiness.

No HIP/HSA compute libraries, hipcc or rocminfo were found. OpenCL has no platform/ICD. The actual Torch GPU allocation fails with `Torch not compiled with CUDA enabled`; AMD ROCm also uses the `torch.cuda` API, but this installed wheel has neither HIP nor CUDA support. KFD access and Vulkan success do not satisfy this missing dependency. A matching RX9070XT/gfx1201 ROCm/PyTorch support matrix, OS/kernel compatibility and isolated GPU-enabled packages are prerequisites for an ML GPU comparison. The follow-up [GPU remedy discovery](gpu-remedies.md) identifies current ROCm10.1 targeted user-space packages. A kernel-driver replacement is not established as necessary; existing-driver compatibility and full storage fit need validation. This lane did not install those packages. A Vulkan tensor implementation is an alternative requiring its own operation/numerical validation, not an equivalent ROCm claim.

## Coherent environment and native closure

The drifted Desktop/Projects/Optiver venv was neither changed nor used as evidence. System Python3.14.4 was used only to orchestrate probes. Existing uv-managed CPython3.12.15 was used to create `/tmp/cgx2-arch-review-environment/venv`; pinned Torch2.11.0+cpu and wgpu0.31.1 were installed there without shared cache writes (`--no-cache` on package installs). All resolved package versions are in `requirements.lock.txt`, executable locations/versions in logs, setup commands in `setup.json`. Total isolated lane size at observation: approximately837MiB. No toolchain rebuild or multi-GB GPU download was needed.

Native C++ compilation uses the wheel's `torch/include`, `torch/include/torch/csrc/api/include`, and `torch/lib`; `-D_GLIBCXX_USE_CXX11_ABI=1`, `-ltorch_cpu -ltorch -lc10`, and an explicit runtime search path. The linker retains the needed ATen CPU/c10 closure; `ldd` resolves all transitive dependencies. The native process independently scans `/proc/self/maps` after tensor work and rejects any loaded libpython. `raw/native-closure.json` hashes resolved ELF libraries; `raw/libtorch-libraries.json` hashes the wheel's shared libraries. The first compile failed because the smoke source omitted `<ATen/Parallel.h>`; that attempt is preserved separately, and the corrected source compiles and runs.

This proves a working **standalone native CPU ATen path without mapped libpython for the exercised operation**. It does not prove that the entire wheel, an authenticated CGx2 dependency package, emitted native executable or arbitrary later operations are Python independent. The wheel contains Python-facing components. CGx2 closure/no-libpython remains **NOT_PROVEN** until its own packaging, admission and executable maps are checked.

## Offline reproduction

Verify saved evidence without downloads or shared environment mutation:

```sh
python3 research/architecture-review/environment/check.py
```

Replay all smoke checks using the retained isolated environment and current actual device access:

```sh
python3 research/architecture-review/environment/probe.py --output /tmp/cgx2-arch-review-environment/replay
```

Run replay outside the sandbox or with authorized elevated device access. The script performs no installation/network operations and builds only in the chosen `/tmp` output directory, with at most one compile at a time and thread counts1. `--python` and `--ocamlopt` allow explicit alternate local paths, but changing versions creates new evidence rather than replaying these pins. Existing isolated tooling is ephemeral: if `/tmp` is removed, first restore the environment with the recorded setup commands and exact pins, then rerun. Rebuilding the environment may need downloads; the offline replay itself never downloads.

The replay exits0 when executable smoke checks pass and the known PyTorch GPU prerequisite remains explicitly `blocked`; a blocked status is not a successful GPU test. `checks.json` preserves individual exits. `result.json` remains partial because the open GPU ML and CGx2 dependency integration rows must remain visible to the next comparison task. See its concrete remedies. No latency, throughput, RSS advantage, architecture superiority, language guarantee or LLM productivity conclusion follows from this evidence.
