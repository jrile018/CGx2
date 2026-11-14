# GPU remedy discovery — metadata only

An isolated ROCm/PyTorch user-space attempt is a concrete next option using the existing driver. **A kernel-driver replacement is not established as necessary.** No GPU wheel, runtime, driver or security change was installed during this discovery. Existing T3 GPU-ML allocation remains blocked; this document supersedes its earlier remedy wording that suggested a required driver/runtime change.

## Release and host distinction

The host is Ubuntu26.04.1 LTS, kernel7.0.0-42-generic, glibc2.43, x86_64, with the RX9070XT/gfx1201 already observed executing Vulkan. `modinfo` identifies an in-tree amdgpu module and returns no version field. `amdgpu`, `amdgpu-dkms` and `rocm-core` packages are absent. Existing render/KFD access succeeds; no permission changes are needed.

The **ROCm10.1.0** Radeon matrix lists RX9070XT/gfx1201, Ubuntu26.04.1/GA7.0 and Ubuntu24.04.5/HWE7.0. Its Radeon driver families are31.60.0/31.50.0. Host OS/kernel/GPU rows match, but the installed in-tree driver has not been identified as either supported vendor family. This is insufficient to claim official support or incompatibility. [Version-pinned10.1 matrix](https://rocm.docs.amd.com/en/docs-10.1.0/compatibility/compatibility-matrix.html?fam=radeon&gfx=gfx1201&gpu=amd-radeon-rx-9070-xt&i=pip&os=ubuntu)

The Radeon/Ryzen page explicitly covers releases through7.2.1; unified documentation begins with CoreSDK7.13. Its7.2.1 OS rows use Ubuntu24.04.4/HWE6.17 and22.04.5/HWE6.8. Those older requirements must not be applied as the current10.1 matrix. [Legacy Radeon matrix](https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/compatibility/compatibilityrad/native_linux/native_linux_compatibility.html)

## Targeted user-space route

AMD documents virtual-environment Python packages and architecture-specific extras. Its current PyTorch instructions explicitly offer `torch[device-gfx1201]==2.13.0+rocm10.1.0` from `https://stable.repo.amd.com/rocm/whl-next/`. The exact cp312 Linux x86_64 wheel exists. CPython3.12.15 is already available locally; no interpreter rebuild is needed. The ROCm10.1 metapackage metadata pins SDKcore, libraries and device-gfx1201 components to10.1.0. [AMD PyTorch instructions](https://rocm.docs.amd.com/projects/ai-ecosystem/en/latest/frameworks/pytorch/install.html), [version-pinned SDK installation](https://rocm.docs.amd.com/en/docs-10.1.0/install/rocm.html?fam=radeon&gfx=gfx1201&gpu=amd-radeon-rx-9070-xt&i=pip&os=ubuntu&ubuntu_ver=26.04.1)

This supports a user-space-only experiment; it does not prove the current kernel driver will run that stack. Missing HIP/HSA libraries do not themselves diagnose a kernel-driver fault. Keep the working driver and existing CPU environment intact, isolate the new environment, and observe the actual HIP/HSA/device error before proposing a driver change. AMD's documented vendor-driver prerequisites still limit an official-support claim. Linux wheel tags alone do not validate native ELF dependencies.

Candidate commands below are **not executed**, and require a reviewed storage allowance first:

```sh
uv venv --python /home/john-riley/.local/share/uv/python/cpython-3.12.15-linux-x86_64-gnu/bin/python3.12 /tmp/cgx2-arch-review-environment/gpu101
uv pip install --no-cache --python /tmp/cgx2-arch-review-environment/gpu101/bin/python --index-url https://stable.repo.amd.com/rocm/whl-next/ 'torch[device-gfx1201]==2.13.0+rocm10.1.0'
```

Resolve and pin the full dependency closure before accepting that install. AMD separately documents `rocm[libraries,device-gfx1201]==10.1.0`; inspect whether Torch extras already select it, rather than blindly adding development tools or all-device packages. Use SDK initialization/testing only inside the isolated environment. Avoid global library-path changes. Then require HIP version present, exact RX9070XT/gfx1201 selection, real GPU int32/float32 kernels, explicit synchronization and checked host readback. A successful install/import is insufficient. CGx2 packaging/native LibTorch GPU integration needs its own checks. Future comparisons should also use matching CPU Torch2.13, rather than treating existing CPU2.11 as an equivalent release.

## Storage evidence and alternatives

HEAD requests returned these exact compressed sizes; no wheel payload was fetched:

| Official AMD artifact | Bytes |
|---|---:|
| Torch2.13.0+rocm10.1.0, cp312 Linux x86_64 |202,600,338|
| rocm-sdk-core10.1.0, py3 Linux x86_64 |419,364,838|
| rocm-sdk-libraries10.1.0, py3 Linux x86_64 |260,814,462|
| rocm-sdk-device-gfx1201 10.1.0, py3 Linux x86_64 |343,467,033|
| Selected four-artifact total |1,226,246,671 (1.142GiB)|

Exact artifact URLs are pinned in `gpu-remedies.json`. This is a selected component sum, **not a resolved dependency-total or unpacked-size estimate**. Standalone Torch `.metadata` returns404, so its dependency closure has not been inspected. The27,536-byte ROCm source metapackage was read in memory for metadata, without building/executing it. HEAD cannot establish unpacked or peak staging usage. A complete install fitting the existing3GiB lane cap is **NOT_PROVEN**. Snapshot free space: `/tmp` is a shared tmpfs with7.71GB free; root has24.52GB free. Concurrent setup can change both. Reassess space and use a separately approved disk-backed scratch allowance if the closure does not fit.

The official PyTorch2.11/ROCm7.2 cp312 manylinux2_28 x86_64 wheel alone is6,221,326,904 bytes (5.79GiB) compressed, exceeding the3GiB cap before extraction. Its filename supports the interpreter/architecture, and host glibc exceeds2.28, but its distinct release matrix does not establish this host's supported integration. Do not choose this route under the current cap. [PyTorch release instructions](https://pytorch.org/get-started/previous-versions/#v2110), [exact legacy artifact](https://download-r2.pytorch.org/whl/rocm7.2/torch-2.11.0%2Brocm7.2-cp312-cp312-manylinux_2_28_x86_64.whl)

The available alternative is the already tested Vulkan/wgpu path for explicitly scoped GPU compute fixtures, retaining its conformance warning and missing tensor/CGx2 integration. A vendor-supported31.50/31.60 environment is another future choice if official support is required or actual user-space tests identify driver incompatibility; it entails separately authorized system work. Neither a container nor a package tag bypasses host kernel-driver requirements.

## Discovery evidence

`raw/gpu-remedies-host.json` records actual `modinfo`, package and disk commands. The six other new raw JSON files preserve official source URLs/body hashes/text, index filenames, successful HEAD responses and failed metadata endpoints. `gpu-remedies.json` hashes all seven. Equivalent read-only checks include `cat /etc/os-release`, `uname -r`, `modinfo -F intree amdgpu`, `modinfo -F version amdgpu`, `dpkg-query -W amdgpu-dkms`, `df -B1 /tmp /home/john-riley`, and `curl -I` on the pinned wheel URLs. Documentation was fetched on2026-11-14UTC; moving latest pages are distinguished from version-pinned10.1 URLs. No GPU package execution, production-support or GPU-ML success is claimed.
