# Explicit foreign routes and source limits

| Route | Reviewed baseline scope | Required full-release acceptance |
| --- | --- | --- |
| Direct LibTorch | Bounded literal vector/add and rank-two f64/f32 matrix source profiles through pinned native facades; native default CLI does not load Python | General typed creation, shape/dtype/device inspection, math, inference, conversion, lifetime/release and an application versus native references |
| Python-only | Explicit allowlisted scalar source declarations through a one-shot isolated Python worker; richer copied values/handles are Rust library APIs | Reviewed pinned interpreter/environment/package from `.cg`, identity drift, exceptions, lifetime and cleanup checks |
| Native C ABI | Bounded declared unsafe scalar/native facade packages, with reviewed binding metadata and loader checks | Complete portable declarations/values and artifact/target/ownership/error/callback/effect validation |
| Persistent process worker | Stage-1 library deliverable and frozen protocol; no general `.cg` or notebook dispatch in this baseline | S2/S3 reviewed operation envelope, no replay/retry after effects, outcome uncertainty, cancellation and session cleanup |

Do not redirect native tensor execution through Python when a native runtime is
missing. Python emission is a separate translation target, not runtime fallback.
Foreign allocation is not CGx2 region memory, and `no_gc` does not suppress Python
GC or LibTorch/device allocation. Raw foreign exceptions must not unwind into
CGx2; opaque handles require declared ownership/release and session lifetimes.

The existing bounded source spellings and lock examples live with their owners:

- [CLI tensor and Python profile](../../crates/cgx2-cli/README.md).
- [Binding checks](../../crates/cgx2-bind/README.md) and
  [real facade preparation](../../crates/cgx2-libtorch/README.md).
- [Explicit Python worker](../../crates/cgx2-foreign/README.md) and
  [copied Python library values](../../crates/cgx2-python-bridge/README.md).
- [Frozen foreign/worker contracts](../../plans/release-completion/contracts.md).

No new first-release syntax is guessed here. Supply the foreign owner's reviewed
package, its manifest/lock, copied artifacts and complete hash/target/version
pins. Then use the existing preview routes, retaining raw JSON observations:

```sh
cgx2-bind check /absolute/reviewed/package --format json
cgx2 check /absolute/reviewed/package --diagnostic-format=json
cgx2 run /absolute/reviewed/package --diagnostic-format=json
```

For the bounded f32 native/Python boundary regression, the owner runner is:

```sh
python3 crates/cgx2-cli/scripts/verify_python_boundary.py \
  --binary-dir /absolute/reviewed/native-binary-directory \
  --real-f32-package /absolute/reviewed/pinned-f32-package \
  --output /absolute/new/boundary.json
```

This is a native boundary/f32 preview check, not the full R08 inference test or
a benchmark. It requires the native owner prerequisites and verified artifact
closure. Absent dependencies mean blocked, and wrong identities must fail
closed. Do not prepare runtime artifacts by installing packages during this
guide. GPU support requires an actual supported device and synchronized device
evidence; no device or unavailable confinement is explicitly unavailable.

`CGX2_PYTHON` may select an explicit interpreter for the current scalar worker;
`CGX2_PYTHON_TRACE=1` exposes successful-call provenance on stderr. The baseline
`local:cpython` identity is unpinned, and isolated `-I -S` excludes ambient site
packages. These settings do not satisfy the pinned Python-package release gate.
The docs checker clears ambient `CGX2*` settings and does not run foreign calls.
R07/R08 remain blocked until supplied package/runtime and integrated source
fixtures are reviewed. No latency, throughput or Python-free inference claim is
inferred from a Rust unit test or a skipped foreign example.
