# Portable foreign schema v2

`ForeignValue` and `Declaration` are dependency-free Rust contracts, not repr(C)
enums or a JSON decoder. Adapters must validate declarations before import and
validate decoded values before publishing them. Current native SDK dispatch is
the bytes/UTF-8/i64-handle subset; lists and other scalar representations are
available for later reviewed adapters, with no source-language support implied.

Values include unit, explicit Boolean byte 0/1, i8/i16/i32/i64,
u8/u16/u32/u64, f32/f64 *bits*, UTF-8, bytes and homogeneous typed lists.
Float bit patterns preserve NaN payloads; this does not promise that a numerical
backend accepts nonfinite values. Lists carry an element type even when empty.
`validate(expected, limits)` checks types, Boolean encoding, total bytes, total
nodes and recursive value/type depth with checked arithmetic. Defaults and hard
ceilings are `max_bytes=1048576`, `max_elements=4096`, `max_depth=32`; callers can
lower them. Empty/unit values cost at least one byte; list nodes count too.
Construction/deserialization itself must be bounded by the caller; validation
does not retrospectively bound an already allocated Rust value.

`Declaration` schema 2 names package/library/exact numeric version, SHA-256,
OS/architecture, `abi=c-sdk-v2`, `calling_convention=c`, symbol, parameter/result
types, ownership/release, `i32_status_zero_ok`, `calling_thread`, callbacks=false,
effects and capabilities. Unknown ABI/thread/callback/effect/capability rules
reject. Effects include foreign/io/allocation/gpu; capabilities native/gpu.
The native preflight narrows these further to exact reviewed symbol contracts.
An `Owned`/`Shared` declaration has a type and release symbol. Native pointers
are private to adapters. `HandleReference` instead carries nonzero session and
resource IDs, type, owner and explicit/final-alias session release policy; it
must validate against the receiving live session and registry. No worker handle
transport or registry implementation is supplied by this crate.

Errors distinguish identity, unsupported, limits, layout, invalid UTF-8,
foreign failure, cancellation and uncertain outcome. The latter two are shared
categories, not a promise of synchronous native cancellation or rollback.
`contain_panic` catches Rust panics inside an extern C facade entry with unwind
enabled; it returns -1. It cannot recover from aborting panics, native faults or
C++ exceptions. C++ facades catch all exceptions before returning status.
