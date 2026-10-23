# Wave 23 native/Python boundary verification

Observed 2026-10-23 in the attached worktree, starting at common plan commit `9b8f6f5610e697429b364528fa32a6b716aa288a`. Linux x86_64; rustc 1.98.1 (48a229cea), Cargo 1.98.1 (797e8a9bc). No root Cargo files or shared design/ledger files changed.

Default `cgx2-python-bridge` now contains Python-free error/limit metadata. The original PyO3 host, copied f64 arrays, tensor/handle APIs, and `Registry::call` require the explicit `in-process` feature. CLI check/run signatures are unchanged. CLI foreign execution uses `ScalarValue` and `Registry::call_worker`, so workspace feature unification does not pull Python-owning value destructors into either native binary. The source allowlist and source types remain unchanged.

## Exact artifacts

| Build/profile | Binary | SHA-256 | Bytes |
| --- | --- | --- | ---: |
| default-debug | `cgx2` | `563cb48fc22489a22dae0ae9c25dbe3ca1e03462cf52b6e4c4a33b64f6195169` | 34,232,192 |
| default-debug | `cgx2-native` | `c23e12946ddc38fe85e41719407ebe28ad8dd2b601f7173a65b205c5d6713a10` | 30,392,992 |
| default-release | `cgx2` | `3fa7f6ff4162871f4733bfe54ac33edead370c12710cb47b2d0de61fcdd97cbe` | 1,981,152 |
| default-release | `cgx2-native` | `2c5b07d84bba3352c76e7706d7f5cf8d6403ab3a6550361c3be66e1985f22b19` | 2,254,720 |
| workspace-all-features | `cgx2` | `39d6d448d6bf1ce2f17a005d6124785f2da183e24093cd2794efe1b96c7661ac` | 36,964,912 |
| workspace-all-features | `cgx2-native` | `e68d6bd346858f91759d2fe887b8e192f61da6563bf05b1d1a8af24f13f42dbb` | 28,872,400 |

The default artifacts came from exactly `cargo build -p cgx2-cli --offline` and `cargo build -p cgx2-cli --release --offline`; dev is unoptimized with debuginfo, release optimized. The unified artifacts came from `LIBRARY_PATH=/tmp/cgx2-wave23-python-link CARGO_INCREMENTAL=0 cargo build --workspace --all-features --offline`. All six ELF artifacts have exactly these `DT_NEEDED` entries: `libgcc_s.so.1`, `libc.so.6`, `ld-linux-x86-64.so.2`. `readelf -d` and `ldd` show no libpython, including for the unified build that also builds the PyO3 host. The restored default artifact hashes were checked again after final tests.

Raw reports: [default debug](evidence/wave23-default-debug.json), [default release](evidence/wave23-default-release.json), [workspace all features](evidence/wave23-workspace-all-features.json). Reports contain each binary's canonical location, size, hash, ELF/ldd output, mapped files, observed child executables, stdout/stderr, and real-package metadata hashes. Temporary input/package paths are observations from this machine, not committed dependencies.

## Runtime observations

| Artifact | Case | Map samples | Outcome |
| --- | --- | ---: | --- |
| default-debug | native_scalar | 19 | completed |
| default-debug | explicit_python | 95 | completed |
| default-debug | real_f32 | 10,998 | stopped after mapped LibTorch; no numerical claim |
| default-release | native_scalar | 3 | completed |
| default-release | explicit_python | 92 | completed |
| default-release | real_f32 | 3,161 | completed |
| workspace-all-features | native_scalar | 19 | completed |
| workspace-all-features | explicit_python | 94 | completed |
| workspace-all-features | real_f32 | 12,408 | stopped after mapped LibTorch; no numerical claim |

Every sampled CGx2/native-helper map lacked libpython. Native scalar execution returned `5000`; explicit Python `math.isqrt(81)` returned `9`. The explicit worker child was `/usr/bin/python3.14`; provenance reported executable `/usr/bin/python3`, CPython `3.14.4 (main, Aug 20 2026, 10:41:58) [GCC 15.2.0]`, prefix `/usr`. Native/LibTorch cases ran with both interpreter and worker overrides set to nonexistent paths and observed no Python children. Native ELF inspection legitimately launches `readelf`, also recorded in the reports.

The completed real release case returned `[[58.0, 64.0], [139.0, 154.0]]`. It used a regular-file schema-2 package at `/tmp/cgx2-wave23-boundary-f32`, real CPU LibTorch 2.11.0, facade ABI 1, f32 API 3, layout `tensor-f32-cpu-matrix-v3`, compiler `/usr/bin/x86_64-linux-gnu-g++-15`, C++ ABI 1, and empty configured Torch flags. Facade SHA-256: `c4a95053537fb52f69f2da2a490b169cff0214d580d00d679b54c85147806bec`. The package contains 454,878,145 pinned bytes. Preparation selected Torch's `libgomp.so.1` explicitly to resolve an ambiguous host alternative. Disk quota prevented another physical library copy; unchanged regular library files were reused by hardlink from a previous copied deployment, then rehashed and pinned for this root. No symlink-to-external-library package was used.

Debug pin verification is slow because it repeatedly hashes the large closure without optimization. Debug/default and unified real cases deliberately stop immediately after observing mapped `libtorch_cpu.so`; they make no completed numerical claim. The separate release real source test passes in 6.92 seconds. Earlier concurrent build/check attempts were discarded after the loader correctly rejected deleted executable mappings; final artifacts were kept stable during observation.

## Checks and results

- `cargo test -p cgx2-cli -p cgx2-foreign -p cgx2-python-bridge --offline`: **95 passed**. Covers existing scalar/memory/record/native cases, float64/f32 mocks, worker scalar boundaries and provenance, malformed request/response tags, UTF-8, truncation/trailing data, wrong identity, byte limits, timeout/crash/reap/recovery, non-reading children, missing worker/interpreter, stdout separation, and first-error UTF-8 call spans for `CGR017`–`CGR021`.
- `LIBRARY_PATH=/tmp/cgx2-wave23-python-link cargo test -p cgx2-cli -p cgx2-foreign -p cgx2-python-bridge --features cgx2-foreign/in-process --offline`: **133 passed**, including the original host/container/keyword/copied-array/handle suites and worker-versus-host error-category comparison.
- Both default and `--all-features` owned `cargo clippy ... --all-targets --offline -- -D warnings`: **passed**. Owned Rust formatting and `git diff --check`: **passed**.
- `LD_LIBRARY_PATH=/tmp/cgx2-wave23-boundary-f32 CGX2_LIBTORCH_F32_SOURCE_PACKAGE=/tmp/cgx2-wave23-boundary-f32 cargo test -p cgx2-cli --release --offline --test tensor real_pinned_float32_matrix_source_when_available -- --nocapture`: **1 passed**, real completed float32 reference result.
- `cargo package -p cgx2-foreign --list --allow-dirty --offline`: **passed**, includes `src/worker.py`. Default package builds need neither PyO3 nor Python development libraries. The worker is embedded, so installation needs no extra helper beyond the existing sibling `cgx2-native` binary.
- System Python has NumPy but no Python `torch`; optional linked-host real PyTorch tests report the unavailable path. This is separate from the completed direct real LibTorch test. Conditional real float64 source tests were not supplied a real package; their mock regressions pass. No GPU acceptance or performance target is claimed.

The system has `libpython3.14.so.1.0` but lacks the unversioned development linker name. In-process verification used only a temporary `libpython3.14.so` symlink in `/tmp/cgx2-wave23-python-link` targeting that exact runtime, with `LIBRARY_PATH` above. This was unnecessary for the default debug/release builds. PyO3 build-time interpreter selection and CLI worker runtime selection are separate.

## Reproduce

Use the [worker contract](../cgx2-foreign/README.md) for discovery, protocol bytes, limits, and packaging. The default interpreter is `python3` on PATH; `CGX2_PYTHON` explicitly overrides it, `CGX2_PYTHON_WORKER` explicitly replaces the embedded script, and there is no fallback. `CGX2_PYTHON_TRACE=1` records successful CLI executable/version/prefix provenance on stderr. The local identity remains unpinned. `-I -S` restricts module search to the selected standard library and disables startup hooks. Overrides are trusted configuration, not attested code.

```sh
cargo build -p cgx2-cli --offline
python3 crates/cgx2-cli/scripts/verify_python_boundary.py \
  --binary-dir target/debug --real-f32-package /absolute/reviewed/f32-package \
  --real-maps-only --output /tmp/cgx2-debug-evidence.json
cargo build -p cgx2-cli --release --offline
python3 crates/cgx2-cli/scripts/verify_python_boundary.py \
  --binary-dir target/release --real-f32-package /absolute/reviewed/f32-package \
  --output /tmp/cgx2-release-evidence.json
```

Omit `--real-f32-package` to record an explicit real-dependency skip. To reproduce unification, build `--workspace --all-features` with a suitable CPython development library, then run the same debug inspector before any rebuild. Recreate a reviewed real package with the existing `cgx2-libtorch/prepare_pinned.py --matrix-f32` workflow; the preparation command is retained in the package evidence, and all three package-evidence hashes are retained in each raw report.

The bounded worker transport currently supports Linux x86_64/aarch64. It bounds frames, strings, argument count and positive execution time up to five seconds, and kills/waits every failed or timed-out child. It does not bound foreign RSS or defend against a malicious selected interpreter/worker. The audited allowlisted worker never forks; hostile administrator overrides are outside that cleanup contract. No general Python packages, array/handle source syntax, persistent worker state, or in-process fallback were added.
