# Wave 21 direct float32 matrix evidence (2026-10-23)

This host used CPU Torch 2.11.0 from
`/home/john-riley/Desktop/Projects/Optiver/.venv/lib/python3.12/site-packages/torch`.
The new facade was built with `/usr/bin/x86_64-linux-gnu-g++-15` on Linux
`x86_64`, C++11 ABI 1, Release `-O3 -DNDEBUG`, and empty Torch CXX flags.
Its link used `$ORIGIN` RUNPATH, `libtorch.so`, `libtorch_cpu.so`, and
`libc10.so`; the recorded build command was
`cmake --build /tmp/cgx2-wave21-f32-build --config Release -j2`.

`prepare_pinned.py --matrix-f32` produced a schema-2 lock for
`tensor-f32-cpu-matrix-v3` with 19 exact symbols (the nine v1 symbols, five v2
symbols, and five v3 symbols). The package was prepared under
`crates/cgx2-libtorch/.wave21-package` for testing. It was removed after
verification; the commands in [PREPARE.md](PREPARE.md) recreate it. The bind
preflight reported `status ok`, `bindings_checked 1`.

| File in pinned closure | SHA-256 |
| --- | --- |
| `libcgx2_libtorch_facade.so` | `c4a95053537fb52f69f2da2a490b169cff0214d580d00d679b54c85147806bec` |
| `libc10.so` | `e8ad028c6a3b3f787b619f35d3c772485476c029c2e9faca659a56212ae7c00a` |
| `libtorch.so` | `59a95807d56ca89993351a173839f160ca9c2c6ec6f6b89a5e691d8f1392d3ff` |
| `libtorch_cpu.so` | `ad0bcc0225bf260968e0464982b78c2f25358400769720ce63c209ffc9929313` |
| `libgomp.so.1` | `78511033caddec6ccae8f4d62b94d56135f377c4cee33120e6d7df2ef499f69f` |
| host `ld-linux-x86-64.so.2` | `bda779ea9e9ad234f60315477d7cfd0bdde861cd8ddd9df8cdaf62f18c4cec14` |
| host `libc.so.6` | `85e64f97e348786a8fb4d9f3d52fec289e2fb86bba20f0731dfe61990525e0f7` |
| host `libdl.so.2` | `59d725ca30596a9863f4fa2552a7125f834842abc86b40890231c129358965d2` |
| host `libgcc_s.so.1` | `9d339ecb409578d6a5d587e6c537a8f9589b8a13fefba30d167433a4b5758bee` |
| host `libm.so.6` | `1c488db6f4fe8cecb72456bc018de000b7a9b539c2994864b0416b5e8371b3d8` |
| host `libpthread.so.0` | `1327fc40710691dbd94e88b6cf4efce27fefe8fbb93f7d1164a75b4dccaea696` |
| host `librt.so.1` | `9f3cc59207bcaaac74aaf7aca961f4816f0f4d945e4bd1c3cb98ec516becf161` |
| host `libstdc++.so.6` | `5bb0d21308f123b6ad46c6f35b42cedfcb8d6d439a53aa3dae04d880aaffdde3` |

The package lock hash was
`51c669ebb5c90105012278d9f479067a8be8171f73a3db44a89761d913ecd768`;
the closure lock hash was
`59ff40d969076326d23b914aa84cc02e2e7dec930831d066726499f8192c2b87`.

The ignored `real_pinned_float32_reference_and_cycles` test passed with
`OMP_NUM_THREADS=1 MKL_NUM_THREADS=1` and the package path supplied through
`LD_LIBRARY_PATH` and `CGX2_LIBTORCH_REAL_PACKAGE`. It ran two complete
create/multiply/copy/release cycles. Its separate scalar reference used
`[2,3] × [3,2]` values and absolute tolerance `1e-5`; reported shape was
`[2,2]` and dtype code `1`. The test took 382.76 seconds because each pinned
operation revalidates the ELF closure. Twelve additional cycles passed with
the mock facade; those are mock evidence only.

AddressSanitizer and UndefinedBehaviorSanitizer instrumented
`facade/f32_asan.c` plus the mock facade, then the same harness plus a newly
instrumented real C++ facade. Both runs passed 12 create/multiply/copy/release
cycles, exact and short copy capacities, guard values, and a `/proc/self/maps`
scan finding no mapped `libpython`. Real Torch dependencies were uninstrumented.
`ASAN_OPTIONS=detect_leaks=0` was necessary because LeakSanitizer could not
operate under this host's ptrace sandbox. The real facade boundary itself was
ASan and UBSan instrumented; the pinned Rust adapter was not.

The remaining lifetime risk is a foreign facade violating its declared handle
contract, or a pinned loader closure becoming unverifiable after a native call:
the adapter then avoids another foreign release call and may leak that handle.
The two-cycle real run is a bounded smoke test, not a proof of long-run lifetime
safety. GPU and `.cg` syntax are outside this lane.
