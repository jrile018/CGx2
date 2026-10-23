# S1-06 delivery evidence

Date: 2026-10-23. Reviewed input: `0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
Branch: `riley/release-worker`. All implementation/evidence writes are within
`crates/cgx2-interop-worker/`. Shared plans, other crates and root manifest are
unchanged. Cargo generated the new workspace-member lock entry during checks;
that generated root lock change was discarded for coordinator reconciliation.
The pre-existing untracked `.serena/` directory is not part of the delivery.

Host: Linux 7.0.0-38-generic, x86_64; Rust 1.98.1
(`48a229cea 2026-09-01`). Actual Landlock ABI: **8**. No package installation,
external service, host-policy change, benchmark or study was performed.

## Commands and actual results

Each Cargo check used this environment in the attached worktree:

```sh
mkdir -p "$PWD/target/release-worker" "$PWD/target/tmp-worker"
export CARGO_BUILD_JOBS=2 RUSTUP_TOOLCHAIN=1.98.1
export CARGO_TARGET_DIR="$PWD/target/release-worker" TMPDIR="$PWD/target/tmp-worker"
```

| Command | Exit | Result |
| --- | --- | --- |
| `cargo test -p cgx2-interop-worker --all-features --offline` | 0 | 16 passed: 2 unit, 3 protocol, 11 transport; no failed/ignored tests |
| `cargo clippy -p cgx2-interop-worker --all-targets --all-features --offline -- -D warnings` | 0 | All owned library, binary and test targets pass |
| `cargo fmt -p cgx2-interop-worker -- --check` | 0 | Owned package only |
| `git diff --check` | 0 | Worktree diff |
| `CGX2_REQUIRE_CONFINEMENT=1 cargo test -p cgx2-interop-worker --all-features --offline restricted_host -- --nocapture` | 0 | Actual filesystem/network confinement required and passed on ABI 8 |
| `cargo test -p cgx2-interop-worker --all-features --offline restricted_host -- --nocapture` inside restricted execution sandbox | 0 | Actual fail-closed unavailable path: child setup/exec denied with EACCES; no unrestricted fallback |

The initial full sandbox run exited 101 because sealed-image execution via
`/proc/self/fd` was denied (EACCES). The final full tests and required-confinement
check ran with child-execution access. Production code keeps the sealed image,
FD isolation and process-group guards in both environments. The unavailable-path
check predates the final API-only/category and type-validation test additions;
its backend and unavailable error path are unchanged.

Persisted final command output:
[tests](evidence/tests.log), [Clippy](evidence/clippy.log),
[mandatory confinement](evidence/confinement-required.log), and
[sandbox unavailable](evidence/confinement-unavailable.log).

## Acceptance coverage

* Deterministic counter state survives separate invocations. A duplicate request
  is rejected and the fsynced write file contains exactly one line. Crash,
  timeout and cancellation after a write retain exactly one effect, return
  `outcome_unknown`, poison the session and prohibit further dispatch.
* Golden big-endian/JSON vector and every JSON truncation; invalid UTF-8,
  version/kind/field mismatch, duplicate object keys (including payloads), raw
  floating numbers, excessive nesting/size, malformed/oversized/truncated child
  frames, wrong session/ID, duplicate terminal response, state overflow and event
  flood are rejected. Canonical base64 and recursive homogeneous list/session
  handle validation have positive/negative vectors.
* Incorrect executable/additional artifact/environment pins fail before import
  or dispatch. Authentication nonce and runtime handshake mismatches fail.
* Nonblocking writes to a child that stops reading respect the deadline.
  Predispatch cancellation/oversize/unknown operation leaves the session ready;
  after-dispatch uncertainty never retries. Valid foreign errors are separately
  represented and the session can continue after them.
* A deliberately inheritable descriptor does not cross exec. An attempted
  `setsid` cannot escape the inherited lifecycle guard. Shutdown, Drop, timeout,
  crash and cancellation kill an actual descendant process and reap the direct
  child. Tests distinguish running processes from host-owned orphan zombies;
  the library does not install a process-global subreaper.
* Restricted probe actually reads an admitted file, fails a write to that file,
  fails reading a different file, fails AF_INET socket creation and fails group
  escape. No network connection is attempted. Unsupported/denied startup fails
  closed and does not claim enforced capabilities.

## Supported scope, limitations and handoffs

This is **library-only transport support**, with a native fixture, on Linux
x86_64 (tested). Linux aarch64 has the explicit syscall-architecture profile but
is unrun; other hosts return unavailable (cross-compilation acceptance unrun).
Restricted mode requires Landlock ABI >=5 plus seccomp/close_range/sealed-exec
support. Its supported resource policy is absolute read/execute roots with no
filesystem writes and no sockets. Writable-resource and network allowlists are
unsupported; callers must inspect actual enforcement rather than infer it from
requested operations. Unrestricted mode has a lifecycle/FD guard, with no
filesystem/network confinement. Neither profile is a general container, CPU/RSS
quota, nor a security guarantee for arbitrary privileged/dishonest runtime code.

Additional package/module/shared-library files are preflight-pinned, not
snapshotted; an immutable reviewed runtime tree remains a consumer prerequisite.
Retained serialized state uses the audited child's accounting and checked report;
it does not measure arbitrary foreign runtime heap bytes. Handle owner/release
metadata is validated but the consuming adapter owns concrete lifetime/release
semantics. These are explicit integration obligations, not source-level claims.

Source `.cg` effects, effectful `.cgnb` execution, shared SDK ForeignValue adapters,
and Node/PHP protocol bootstrap/package/lifetime acceptance remain **unrun and
assigned downstream (S2/S3)**. Existing native/Python routes are unchanged. The
[README](README.md) specifies the exact wire schemas, lifecycle and API handoff.
For real Node/PHP acceptance a consumer must supply immutable pinned runtime and
bootstrap artifacts and a wrapper using these exact schemas, then repeat the
counter/write/crash/cancel/cleanup suite without replaying prior notebook cells.
No such runtime adapter is claimed by this commit.

Quiet-host performance studies and non-Linux/aarch64 acceptance are **unrun**.
The README contains executable mandatory-confinement and full-check commands for
a capable runner; no skipped external check is presented as a pass.

New crate dependencies use versions already present at the reviewed baseline:
`serde=1.0.229` (derive), `serde_json=1.0.149`, `sha2=0.10.9`, and Linux-only
`libc=0.2.189`. Coordinator action: regenerate the root workspace lockfile to add
only the new `cgx2-interop-worker` member and these existing dependency edges.
