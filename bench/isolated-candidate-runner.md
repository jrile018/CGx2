# Isolated scored submissions (Wave 15)

Run `python3 bench/scored_candidate.py --probe` before preparing any candidate.
The probe checks Linux Bubblewrap user, mount, PID, network, IPC, and UTS
namespaces, libseccomp, Landlock, and live
filesystem and network denial. An unsupported host returns JSON with
`"status":"unsupported_isolation"`, exits 2, and does not compile or run a
submission. The private `accept.py --trusted-local-smoke` remains a trusted
fixture check and is never scored isolation.

For an independently reviewed and frozen task, use:

```sh
python3 bench/scored_candidate.py \
  --pack bench/fixtures/task-candidates/v3 \
  --task quote-credit-v3 --language rust \
  --candidate-workspace /path/to/submitted/workspace \
  --toolchain-root /path/to/read-only/toolchain \
  --compiler-relative bin/rustc
```

The compiler must be inside the toolchain root. Its SHA-256 must equal the
frozen `protocol.compiler_sha256` for CGx2 or `protocol.rust_compiler_sha256`
for Rust. The task needs status `frozen`,
an accepted review card, all protocol pins, and matching starter, prompt,
and hidden-oracle hashes. The current candidate pack has no independent
approvals or frozen protocol pins; it returns `unrun` before launching.

The host oracle generates each call and checks output only after the worker
exits. Candidate-visible read-only `/workspace` holds a copied submission,
the generated call, and worker configuration without expected answers. The
host repository, private test source, task cards, home, and reference overlays
are absent. The worker gets read-only `/toolchain` and system libraries,
private tmpfs `/scratch` and `/tmp`, a cleared environment, UID/GID 65534,
no capabilities, and no network. An outer seccomp filter blocks network and
privileged syscalls. After compilation, Landlock permits execution only of
the compiled result and ELF interpreter; another seccomp filter blocks
process creation. CPU, wall time, address space, file size, process count,
open files, tmpfs, and output have bounds in `isolation.py`. CPU, address-space
and process-count limits are currently per process; they do not establish an
aggregate cgroup limit for a compiler that creates children. Keep scored
evidence gated until total resource containment is demonstrated.

The JSON evidence records profile settings, compiler, worker and isolation
hashes, copied workspace hash, pack/card/oracle hashes, capability probe,
case input/output hashes, and pass/fail reason. Keep it outside model
workspaces and prompts. A valid profile does not replace independent task
review or genuine paired model runs.

On a capable worker, run the positive and adversarial checks before scoring:

```sh
python3 bench/isolation_selftest.py \
  --toolchain-root /path/to/read-only/rust-toolchain \
  --compiler-relative bin/rustc
```

These fixtures attempt private reads through `std::fs` and `openat`, process
execution, network connect, symlink traversal, memory exhaustion, and output
flooding. On an unsupported host the self-test returns
`unsupported_isolation` without compiling a fixture.
