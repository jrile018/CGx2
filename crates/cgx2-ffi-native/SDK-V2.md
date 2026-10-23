# Native SDK v2

`sdk_v2::Sdk::open_pinned(root, binding_id)` is the unsafe, reviewed entry;
subsequent typed methods are safe under `sdk_v2.h`'s contract. The adapter is
Linux/x86-64, calling-thread-only (`Rc`, neither Send nor Sync). It retains the
checked artifact file and DSO through all functions/handles/aliases. Package,
target, hash, exports and exact layout check before loading; metadata identity,
version, ABI=1 and SDK API=2 check before operations. Native dependencies must
also be reviewed; they are not transitively hashed by this profile.

`copy(input, utf8, limits)` borrows input for one call, allocates at most
`limits.max_bytes` output capacity (default/hard ceiling 1 MiB), calls once,
checks status and `written <= capacity`, and returns independent owned bytes or
strict UTF-8. Embedded NULs are supported. Invalid UTF-8, overreported sizes and
foreign errors never publish a partial value. It never probes size or retries
an effect. Inputs larger than the limit reject before allocation/dispatch.
Caller pointers must not be retained; output is always caller-owned, so there
is no foreign result buffer requiring a later free. A lying C function that
writes past capacity is an ABI violation at the unsafe review boundary.

`create(i64)` returns one `OwnedHandle`; `read()` borrows it. Each create must
return a distinct owner. A per-thread mapped-inode registry detects aliases
across independently opened SDK adapters too. Nonnull failure outputs are
released only when distinct from live owners and the artifact is still valid.
`release(&mut self)` is terminal once invoked, **including nonzero foreign
status**. Later release/read rejects; Drop never retries an already attempted
free. This v2 rule deliberately differs from the preserved v1 retry-on-status
contract. `into_shared()` transfers ownership to immutable Rc aliases;
`SharedHandle::clone` does not call foreign retain, and the final alias Drop
releases once. No raw pointer escapes, callbacks or foreign retention exist.

Pin drift rejects subsequent calls. If cleanup cannot verify the artifact, Drop
declines to invoke foreign code and may leak the quarantined foreign resource;
it does not assert successful cleanup. Caller unwinding otherwise drops owners
normally. C++ catches exceptions and Rust catches panics **inside** the facade;
the host cannot safely catch a foreign unwind after it crosses C. Native calls
are synchronous, with no cancellation, timeout, rollback or crash isolation.

Stage 2 integration recipe: admit explicit reviewed foreign declarations with
foreign/native effects/capabilities; resolve only an allowed schema-3 binding;
map bytes/string results to portable values; lower unique/shared scopes to
these owners, never pointer-sized integers; use drop guards for every error or
unwind edge; reject retaining callbacks/threads and implicit Python fallback.
Keep package pins and source/effect provenance in the checked package. The
current deliverable supplies Rust APIs and packaging examples only. Compiler,
`.cg` strings/bytes/handles, notebook retention and worker adapters need their
separate reviewed S2/S3 integration. Scalar native source calls remain on v1.
