# Frozen contracts for parallel completion

## Stage 1 compatibility

All 12 tracks start from the same committed reviewed baseline. Existing Module/Function/ExprKind/StmtKind/Type/Value forms, signatures, diagnostics and lock schemas are frozen. In particular retain CheckedPackage, CheckedModule text/syntax/typed/source_sha256/canonical_path access, verify_unchanged, ImportedSignatures and checked imported effect trust rules. No lane adds required struct fields or exhaustive enum cases consumed elsewhere. Additive versioned modules/builders are allowed. Native SDK lock extensions use a new version; existing formats remain readable and conservative.

Core owns source semantics and compiler integration; libraries cannot claim source support from unit tests. Notebook owns evaluator/state integration. Only coordinator owns root Cargo lock and shared contract changes.

## Foreign value/declaration contract (new version only)

Stage 1 native SDK implements the shared representation; Python/worker lanes preserve their current public types and use adapters after review. No cross-lane unpublished code dependencies.

Portable copied values: unit, explicit Boolean encoding, fixed-width signed/unsigned integers, finite-width float bit patterns, UTF-8 strings and bytes, bounded homogeneous lists. Opaque handles never cross process boundaries as raw addresses: worker references are session-scoped IDs with declared type/owner/release policy. Record/variant source lowering is S2. All length/size arithmetic checked; recursion/nesting bounded. No implicit zero-copy, callback retention or managed session sharing.

Declarations identify schema version, package/library version and artifact hash, OS/architecture, ABI/calling convention, allowed symbol, parameter/result representations, ownership/release, error convention, thread/callback rules, effects/capabilities. Pinned environment validation precedes import/call. Unknown or mismatched identity/layout/lifetime fails closed. Raw foreign exceptions do not unwind into CGx2. Preserve each current error API; additive errors distinguish identity, unsupported, limit, foreign failure, cancellation and uncertain outcome.

## Persistent local-child wire contract v1

Transport is a private spawned-child channel, not a listening network service. Parent creates an unpredictable session nonce, passes it via a private inherited channel, and expects a matching handshake plus configured runtime identity. Do not leak it in command-line args/logs. The parent verifies executable/environment identity before executing code; the handshake is not sufficient identity proof.

Frames: four-byte unsigned big-endian length then UTF-8 JSON envelope, maximum 1 MiB frame by default, nesting ≤32, strings/bytes and list lengths charged to the same frame/state limits. Bytes use explicit base64 encoding; floats use explicit tagged bits rather than JSON NaN. Envelope fields: schema_version=1, session_id, request_id (monotonic nonzero unsigned integer after handshake), kind, payload. Kinds include hello, invoke, result, error, cancel, cancelled, shutdown. Unknown fields/kinds fail unless explicitly versioned as extensible. Diagnostics/stdout are bounded explicit events and cannot corrupt framed responses.

One in-flight invocation per session initially. Request timeout default 2000 ms, configurable positive bounded duration. Maximum retained serialized state default 1 MiB; library permits explicit configured limits with checked validation. No automatic retry after dispatch. Duplicate IDs reject. Once a request may have performed an external effect, a timeout/crash is outcome_unknown, never “rolled back”; poison/terminate the session unless a reviewed protocol proves safe resumption. Cancel/shutdown must clean up descendants. Completed requests can be acknowledged once; transport is at-most-once per live session, not an exactly-once guarantee across crashes.

Capability configuration lists allowed operations/resources plus required confinement mode. Unrestricted local child and restricted child are visibly different modes. Restricted mode checks actual host enforcement; unavailable confinement returns unavailable. An allowlist is never represented as OS confinement. No host policy modifications.

A fixture worker with persistent counter demonstrates state and duplicate suppression. This does not itself implement notebook effects.

## S3 notebook execution contract

Supported external-effect cells are explicitly admitted with capabilities. A selected execution sends one request; earlier effectful cells are never replayed to reconstruct state. An explicit user rerun is a new invocation; clean restart/run-all starts a new session and intentionally executes cells in order.

Publish notebook bindings/output only after a validated result, but report that external effects cannot generally be rolled back. A crash/timeout after dispatch marks result unknown/stale and requires an explicit restart/recovery action; no silent retries. Source edits invalidate dependents; uncertain effects invalidate later cells conservatively. Persistent foreign handles remain session-bound and are invalid after restart. Worker/kernel own cleanup on cancellation/exit. Rich output sanitization, provenance and content-addressed assets retain existing limits.

S2/S3 integration chooses the concrete compiler/evaluator operation envelope only after the core type contract is reviewed; Stage 1 worker must not guess arbitrary .cg execution semantics.

## Checkpoint policy

If implementation proves a frozen contract impossible or unsafe, report a precise proposed amendment and affected consumers. No worker changes shared docs or another lane. The coordinator reviews amendments before subsequent dependent chats start.
