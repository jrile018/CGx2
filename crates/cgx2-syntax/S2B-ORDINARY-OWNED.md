# Ordinary-owned source functions, schema 1

This is the first executable local ordinary-owned function slice. The full
ordinary-memory checkpoint, S2b, S3, S4 and the release remain incomplete.
The legacy graph-script guard/report repairs remain separate defects on this
base; this implementation does not change or consume those repair interfaces.

## Source contract

The source producer accepts local nongeneric nominal records with `i64` and
`Bool` fields, copied scalar/record/Unit values, and `owned<R>`, `shared<R>` and
`exclusive<R>` function parameters/results. `owned(record_value)` allocates;
`borrow(owner)` and `borrow_mut(owner)` borrow a named whole owner. A borrowed
result must derive from exactly one eligible borrowed input of the same layout.
`main` takes no parameters and returns a copied value or Unit. All local
functions, including unused functions, must check successfully.

Functions support positional mixed copied/ownership arguments, nested local
calls, whole-owner moves and returns, named-base `p.field` reads and exclusive
writes, mutable copied scalar locals, `if/else`, early returns, comparisons and
checked i64 arithmetic. Record constructor arguments name fields and evaluate
in source order, even when different from layout order. Copied record
parameters and results keep their copied representation; they do not become
fake ownership values. Owners have no clone-on-lookup path. Source argument
evaluation transfers an owner before evaluating the next argument.

Loans have lexical function/if-block lifetimes. Direct `f(borrow(x))` and
`f(borrow_mut(x))` arguments have call-only lifetimes unless a checked borrowed
result prolongs them. Inline call-only access starts at the call; copied scalar
argument snapshots evaluate first. Named shared loans can pass to checked
shared parameters. Forwarding or aliasing an existing exclusive loan rejects
with `CGT083`; the compiler never ends a live loan just to permit a call.
Post-call projection such as `borrow(x).field` remains unsupported syntax;
bind a named loan first. Returning a loan to a local/consumed owner rejects.

Ownership wrappers select the new source profile before copied evaluation.
Unresolved intrinsic calls also select it; ordinary functions/records named
`owned`, `borrow` or `borrow_mut` outside this profile retain ordinary
resolution. Within the profile, ownership wrapper/intrinsic declarations and
local/parameter shadowing reject with `CGT082`. Intrinsics are admitted only
after this declaration policy has been checked. Bare copied nominal types named
`owned` do not select an applied ownership wrapper.

Imports/packages, generics, recursive SCCs, variants, loops/matching, partial
moves, broader reborrows, named function-call arguments, contracts, declared
effects/capabilities, foreign calls, strings/collections, managed values,
regions, explicit conversions and `no_gc` are diagnosed before execution or
artifact replacement. Their full ordinary-memory semantics remain mandatory.

## Checked representation and authentication

`cgx2-sema/src/ordinary_owned.rs` exposes `compile_source(&str) -> OwnedProgram`
and `SCHEMA_VERSION = 1`. Its private fields retain the exact parsed module,
exact UTF-8 source, source SHA-256, a deterministic module SHA-256, typed model,
all ownership CFGs and body-checked summaries. There is no public constructor,
mutable acceptance bit or unchecked summary import/deserializer. Public model,
CFG and summary accessors return shared references. Cloned/fabricated copied
`TypedModule` flags cannot authorize owned Rust or Python emission.

`authenticate`, `execute`, `execute_with_session` and `rust_data` rebuild from
the stored source and compare every retained representation and digest. Each
rebuild requires `cgx2_ownership::v1::analyze_function(...).is_accepted()` using
unchanged `AnalysisLimits::default()`. A callee summary becomes available only
after its entire body passes. Recursive ownership SCCs reject before summaries
are exposed. Mismatched source, layouts, bodies, CFGs, summaries or digests
reject with `CGT084`; whole-module evidence errors use the original module span.
Unit tests deliberately forge these private fields to verify rejection.

The model in `cgx2-sema/src/ordinary_owned/model.rs` identifies layouts,
functions and source locals by stable declaration/slot indices in the module;
loan tokens have deterministic function-local identities in the retained v1
CFG. Functions, locals, expressions, statements and layout fields retain UTF-8
byte spans. Field indices resolve against a checked local nominal layout;
ownership places retain typed field projection names. Callee identity, result
provenance and the complete body-checked summary set are authenticated together.

`Function::ownership_parameters` maps **source parameter index to optional v1
index**. Copied parameters remain in typed/runtime source order but are omitted
from the v1 parameter/local prefix. Only owned/shared/exclusive parameters
occupy that prefix; owner temporaries append afterward. For example
`(i64, owned<R>, Bool, owned<R>, i64)` maps to
`[None, Some(0), None, Some(1), None]`. A returned second owner uses
`ResultMode::Owned { from_parameter: Some(1) }`. Borrowed-result source indices
are remapped the same way across nested calls, including a borrowed result at
ownership index 1 after an unrelated consumed owner. Copied results map to
`ResultMode::Unit`; ordinary typed IR still carries their real result type.

The lowering builds real branch/return CFGs. Lexical loan ends are explicit;
owned slot cleanup is implicit at scope/return and implemented by the shared
engine. The v1 pass conservatively retains owner state until exit where earlier
destruction cannot invalidate any admitted escaping loan. Source scopes cannot
assign an inner loan into an outer binding. Default v1 limit exhaustion remains
`CGO106`, tested with source exceeding the owner-local bound.

## Storage, execution and emission

Layout fields pack in declaration order, without padding: `i64` occupies eight
signed little-endian bytes; `Bool` occupies one byte and only 0/1 are valid.
Offsets use checked addition. There are at most 256 fields per record and 256
functions; v1 applies its existing CFG/local/operation/source bounds. Layouts
contain no pointers or nested ownership. Invalid nominal fields/types/layouts
fail before allocation. Forged offsets cannot reach execution/emission. Byte
encoding and invalid Bool decoding have separate storage unit tests.

`cgx2-sema/src/ordinary_owned/engine.rs` is compiled unchanged into the
interpreter and emitted Rust. It uses `Session::try_new`,
`Session::alloc_owned_bytes`, `OwnedBytes::as_slice` and
`OwnedBytes::as_mut_slice`. Owners are unique arena slots containing actual
`OwnedBytes`; named owner lookup takes its binding. Loans carry a checked
storage identity, never cloned owner data. The engine drops actual buffers at
scope/return and function unwinding, then drains residual temporaries on an
error. Vacant arena slots are reused across repeated calls. Copied record
snapshots are ordinary copied values, not the owner's storage.

Execution is bounded to 1,000,000 steps and call depth 64. Checked arithmetic,
session entry and owned allocation failures are uncaught `CGR080` failures with
source spans. Earlier mutations are not rolled back. Recoverable ownership-
bearing `Result` and general cancellation/contract/unwind closure are not
implemented. The `failure-injection-test` sema feature and existing CLI/native
fault switches provide synthetic allocation-fault tests, not a real allocator
exhaustion or leak proof.

`CGX2_OWNED_EVIDENCE=1` prints a schema-1 JSON line to stderr on interpreter and
native execution, including failure. It counts actual owned allocations,
releases, payload bytes, peak live payload bytes and byte-storage reads/writes.
These are **payload accounting**, excluding runtime metadata (the runtime's
logical owner header is `OWNED_METADATA_BYTES = 32`) and host evaluator/compiler
bookkeeping. This does not implement the general `memory report` ownership
accounting path or measure RSS/allocator leaks. A source allocation failure
releases all earlier owners. The repeated-call fixture allocates/releases 100
eight-byte owners with peak payload eight bytes, demonstrating scope cleanup
rather than retaining all buffers until program exit.

`cgx2 check`, `run`, `build` and `emit --target rust` select this producer.
The older `check_source -> TypedModule` API stays a conservative copied-value
consumer; `validate_owned_source` and `compile_source` are the new source entry
points. `cgx2-emit-rust::emit_owned_rust(&OwnedProgram)` authenticates before
publishing code. Its generated program embeds the same typed model/engine and
source/module digests. The native compiler stages/hashes its existing complete
nine-file runtime inventory, including `graph.rs`; standalone Rust requires
the matching `cgx2_runtime` rlib and the runner stages it independently.

Python rejects valid ownership-observable source with `CGE006` before replacing
an artifact, even with `--force` or forged copied finite-value flags. Invalid
owned source gets its static error first. Foundation source/emission outside
this profile retains its existing path. Package/API copied consumers reject
`CGT080`; notebooks reject `CGNK026`; effect/security analysis rejects
`CGI006`; verifier output is `unknown`/`unsupported`, never a proof.

## Reproduction and evidence

The bounded product runner is `ordinary-owned-evidence/run.py`. It commits real
`.cg` fixtures, exact negative diagnostic codes/UTF-8 spans in `expected.json`,
interpreter/native-debug/native-release/independent-Rust parity, actual storage
accounting, failure cleanup, and artifact-preserving Python/static rejection.
It bounds each process to 120 seconds wall time, 60 seconds CPU, 2 GiB virtual
memory, 1 MiB retained output and two linker threads. The first attempt exposed
the host linker's excessive default thread creation under a memory cap; the
two-thread wrapper is retained in reproduction and is not a timing result.

`ordinary-owned-evidence/verify.py` runs the full offline locked all-feature
workspace tests, all-target/all-feature Clippy with warnings denied, fmt/diff,
all seven foundation runners, the unchanged legacy graph99 runner, and the new
product runner. Exact commands, exits and log hashes are retained in the owned
evidence directory. Root Cargo files and legacy repair files are not committed.

The owned sema manifest adds only the already-local `cgx2-runtime` path and
already-cached `sha2 = "0.10"` dependencies. A **two-entry root-lock
reconciliation** is necessary before `--locked` builds from this commit:
`cgx2-sema` needs `cgx2-runtime` and `sha2` in its dependency list. Verification
uses that local reconciliation; the root lock is restored before committing.
The exact patch is retained as `ordinary-owned-evidence/lock-reconciliation.patch`
for coordinator review. There is no downloaded dependency or root manifest
change. After local lock reconciliation, reproduce with:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-ordinary-owned"
export TMPDIR="$PWD/target/tmp-ordinary-owned" LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
python3 crates/cgx2-syntax/ordinary-owned-evidence/verify.py \
  --output-dir "$TMPDIR/reproduction"
```

The final run passed **1,173 workspace tests** in **258 suites**, with six
ignored; Clippy, formatting and diff checks passed. The seven unchanged
foundation runners passed 159/278/207/118/88/31/104 checks respectively. The
new runner passed **256 checks** (seven executable source cases and 15 rejected
source cases); the unchanged graph runner passed 99 supporting checks. Separate
injected native allocation failures in debug/release released the earlier owner.
The existing output-bound runner required its supported 4 GiB optimized-build
cap after rustc exhausted 2 GiB; both attempts are retained. The two legacy
review repros still demonstrate their original defects, regardless of graph99.

`ordinary-owned-evidence/evidence/verification.json` records final command exits,
counts, source hashes, dependency reconciliation, preliminary failures and
exports of exact logs. `.gz` exports preserve original bytes with SHA-256 hashes;
`gzip -dc FILE.log.gz` reads them. `product.json` contains every bounded product
command, output, storage observation, runtime inventory hash and UTF-8 span
assertion. This is test evidence for the specified subset, not an independent
safety audit or leak-enabled release acceptance.

## Mandatory remaining memory ledger

All remain required: generic/recursive/package ownership summaries; loops,
matching, partial moves and reborrows; ownership-bearing Result provenance and
recoverable transfers; default managed ordinary records/variants/List/Array/
String, aliases, cycles and root safepoints; named regions, explicit conversions
and full session-wide `no_gc` accounting; contract/allocation/fault/cancellation/
unwind cleanup closure; source/target and consumer closure; external leak-enabled
acceptance and independent safety audit. Typed foreign/package/tensor/execution,
command closure, S3 and S4 remain required. Every R01–R15 gate remains mandatory.
Neither this slice nor its passing checks closes ordinary memory or the release.

The smallest next memory extension is a versioned summary for allocation
`Result` arms (success owns the new scalar record; failure owns no new value),
followed by source-level recoverable allocation for these same fixed layouts.
That requires an ownership-interface review before dependent consumers proceed.
