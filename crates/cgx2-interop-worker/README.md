# Persistent local-child transport v1

This additive library implements the S1-06 transport from reviewed base
`0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`. It does not evaluate `.cg`, execute
notebook cells, or replace the existing Python/native routes. Notebook, Node and
PHP integration is a later consumer of `cgx2_interop_worker::v1`.

## Integration boundary

A consumer supplies `Config` with an absolute executable `Artifact` and its
**previously reviewed** SHA256, pinned bootstrap `arguments`, additional file
`artifacts`, an exact `BTreeMap<String,String>` environment and its reviewed
`environment_sha256`, `runtime_identity`, `CapabilityPolicy`, and `Limits`.
`environment_digest` hashes the canonical sorted JSON string map; it is a pin,
not a signature. Never derive production expected hashes from whatever happens
to be installed at launch. There is no PATH lookup, inherited environment,
installation, interpreter fallback or connection to a service.

`Session::spawn(config)` verifies pins before execution. Executable bytes are
copied into a sealed memfd and executed from that immutable snapshot. Each
additional artifact is preflight-hashed (maximum 256 MiB per file); the consumer
must keep module/package/shared-library trees immutable for the session. The
library does not snapshot an interpreter's entire dynamic dependency tree.
A caller selecting Node/PHP must pin their executable and bootstrap/module
artifacts explicitly, use a protocol-compliant wrapper and isolate ordinary
runtime stdout into explicit protocol events. The fixture is a native ELF child;
interpreter adapters and their real package acceptance remain unrun.

After startup:

```rust,ignore
let mut session = Session::spawn(config)?;
// Preserve these separately in notebook/execution metadata.
let requested = session.policy().clone();
let enforced = session.enforcement();
let cancel = Cancellation::default(); // clone to a cancellation/controller thread
let output = session.invoke(1, Invocation {
    operation: "declared_operation".into(),
    arguments: vec![WireValue::I64(7)],
}, &cancel)?;
// output.result is a copied value OR a validated ForeignFailure, with bounded events.
// Publish bindings only after this returns successfully and the adapter checks its type.
session.shutdown(); // Drop is also an unconditional cleanup path.
```

IDs must strictly increase, are nonzero, and are scoped to `session.id()`.
`&mut Session` permits only one in-flight invocation. Validation failure before
dispatch leaves the ID reusable. Once sending starts the ID is consumed.
No request is retried, including after a short write, cancellation or child crash.
`WireValue::validate_for_session(session, limits)` is available to wrappers.
Opaque handles carry IDs, declared type, owner and `explicit`/`session` release
policy; they do not implement remote resource release by themselves.

## Wire profile

Every frame is a four-byte big-endian unsigned length followed by one UTF-8 JSON
envelope. The exact envelope fields are `schema_version:1`, `session_id`,
`request_id`, `kind`, `payload`. Unknown fields, duplicate object keys, unknown
kinds, trailing garbage, invalid UTF-8, wrong identity and malformed payloads
fail closed. JSON nesting, including envelope/payload objects, is at most 32.
Raw JSON floating numbers are rejected; `f32_bits`/`f64_bits` use unsigned integer
bit patterns, including NaN payloads, without JSON NaN. Bytes are canonical padded
RFC4648 base64. All list elements have one compatible recursive type (empty lists
infer from other elements). Strings/lists/base64 are charged by serialized bytes.

`Limits::default()` sets `frame_bytes=1048576`, `state_bytes=1048576`, and
`timeout_ms=2000`. Explicit frame sizes are 256..16777216 bytes, state sizes
1..16777216 bytes, and timeout 1..60000 ms. Every frame is checked before receive
allocation, outbound serialization is bounded, and nonblocking pipe reads/writes
check the same request deadline and cancellation signal. At most 64 explicit
stdout/diagnostic events and one frame budget of aggregate event text are retained
per request. Raw stderr is discarded; raw stdout corruption poisons the session.

The parent sends `hello` with request ID 0 and payload fields `nonce`,
`runtime_identity`, `limits`, `policy`; the child must echo it exactly. Nonce and
session ID are independently generated 256-bit values. The secret nonce travels
only in private child stdin/stdout, never arguments/environment/error logs.
Identity relies on the executable/environment preflight as well as this handshake.
The configured child is a trusted protocol implementation, not an untrusted peer
that gains credibility by merely echoing a nonce.

Payload schemas (all strict):

| Kind | Payload |
| --- | --- |
| `invoke` | `{operation, arguments:[WireValue...]}` |
| `result` | `{value:WireValue, state_bytes:unsigned}` |
| `error` | `{code:string, message:string, state_bytes:unsigned}` |
| `stdout`, `diagnostic` | `{text:string}` |
| `cancel`, `cancelled`, `shutdown` | `{}` |

A tagged `WireValue` is `{type:"i64",value:7}` (likewise `bool`, `u64`,
`f32_bits`, `f64_bits`, `string`, `bytes`, `list`, `handle`); unit is
`{type:"unit"}`. Result/error `state_bytes` reports the entire retained serialized
state. The child must honor the negotiated state budget; the parent rejects
reports above it. This is protocol accounting, not measurement or confinement of
arbitrary child RSS/foreign allocations. Future adapters must audit that accounting.

## Lifecycle and cleanup

```text
spawn + pins + authenticated hello -> Ready
Ready + valid new ID              -> InFlight(id)
InFlight + validated result/error -> Ready
InFlight + any transport failure  -> Poisoned + kill group + reap child
Ready + unsolicited frame         -> Poisoned + kill group + reap child
Ready + shutdown / Drop           -> Closed + kill group + reap child
```

`Error::OutcomeUnknown` (`category() == "outcome_unknown"`) means an external
effect may have occurred. An observed cancellation after dispatch still has this
status, even if a child emits `cancelled`: it is not evidence of rollback.
The control `cancel` is best-effort before forced cleanup. Shutdown is also
best-effort and does not depend on the child's cooperation. Poisoned sessions
cannot be resumed. A new session never turns an unknown invocation into a retry;
recovery/rerun must be an explicit higher-level action.

Linux x86_64/aarch64 is the implemented host profile. All children start a new
process group. An inherited seccomp lifecycle guard denies `setsid`, `setpgid`,
`unshare`, and `setns`, so descendants cannot detach from cleanup. All parent FDs
above stderr become CLOEXEC using `close_range`; the private stdio pipes remain.
Cleanup sends SIGKILL to the entire group and reaps the direct child. Descendant
zombies are left to the host's init/subreaper; tests verify no descendant remains
running. A host that cannot install the guard or execute sealed images returns
unavailable; it never weakens cleanup. The library does not alter process-global
subreaper settings, cgroups or host policy. Non-Linux `Session::spawn` returns
unavailable; cross-compiled acceptance is unrun.

## Requested policy versus real enforcement

`policy()` returns the requested operation/resource lists. Parent-side admission
of operation names is advisory against a dishonest implementation and is not OS
confinement. `enforcement()` is separate:

* `Unrestricted`: filesystem and network are unrestricted; only the mandatory
  lifecycle/FD guard applies.
* `LinuxReadOnlyNoNetwork { landlock_abi }`: actual child-only Landlock ABI >=5
  denies filesystem writes, truncation, directory mutation and device ioctls;
  `resources` are absolute read/execute roots. Read access outside them is denied.
  Seccomp denies socket/socketpair creation, io_uring setup and relevant escape
  mechanisms. Kernel architecture checks prevent alternate syscall-ABI bypasses.

Restricted mode is intentionally a narrow read-only/no-network profile. It does
not promise arbitrary writable-resource or network allowlists. A caller needing
those capabilities must treat this profile as unsupported and supply a future
reviewed backend rather than choosing unrestricted silently. Runtime loader files
must be explicit read roots. Anonymous sealed executable memory is outside
Landlock path mediation. `PR_SET_NO_NEW_PRIVS`, Landlock and seccomp are installed
in the child between fork and exec; filters and path FDs are prepared before fork.
Missing kernel support, unsupported architecture, denied setup or denied exec
returns `Error::Unavailable`; no fallback is attempted.

## Executable acceptance handoff

See [EVIDENCE.md](EVIDENCE.md) for actual results. From the attached worktree:

```sh
mkdir -p "$PWD/target/release-worker" "$PWD/target/tmp-worker"
export CARGO_BUILD_JOBS=2 RUSTUP_TOOLCHAIN=1.98.1
export CARGO_TARGET_DIR="$PWD/target/release-worker" TMPDIR="$PWD/target/tmp-worker"
cargo test -p cgx2-interop-worker --all-features --offline
cargo clippy -p cgx2-interop-worker --all-targets --all-features --offline -- -D warnings
cargo fmt -p cgx2-interop-worker -- --check
git diff --check
# A capable Linux runner must run this without an unavailable acceptance escape:
CGX2_REQUIRE_CONFINEMENT=1 cargo test -p cgx2-interop-worker --all-features --offline restricted_host -- --nocapture
```

A restrictive execution sandbox may require a child-execution-capable runner.
No package installation or security-policy change is needed. The fixture uses
persistent counter state plus fsynced writes to prove no transport replay; the
filesystem/network probe makes real OS calls without connecting to any service.
Source/notebook effects, Node/PHP package identity/lifetime adapters, shared
ForeignValue mapping and real performance studies remain assigned downstream.
