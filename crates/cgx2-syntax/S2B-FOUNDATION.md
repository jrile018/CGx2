# S2b generic/data foundation

This is the bounded R02/R10 foundation, not completion of S2b or a release.

## Language contract

Generic binders belong to the canonical module/declaration identity plus `::T`.
Type application uses `f<i64>(value)`, `Box<i64>(value: 1)` and
`Choice<i64>.Some(1)`. Calls and record/variant constructors infer arguments
from payloads and an annotated expected result when unique. Unbound arguments
are errors; there are no implicit defaults or heterogeneous collections.

The supported constraints are exactly `Eq` and `Show`. `Eq` permits `==`/`!=`;
`Show` permits `show(value)`. i64, Bool, String, Unit and finite homogeneous
List/Array/record/variant values satisfy each constraint structurally when all
children do. A type parameter supplies only its declared constraints. Arithmetic
and ordering still require i64. Unknown or duplicate constraint names reject;
there is no user-defined trait implementation or implicit dispatch ecosystem.
Constraint satisfaction memoizes canonical types, with at most 4,096 visited
types. Local functions retain precedence over same-named data intrinsics; their
real bodies still determine effects and constraints.

List<T> and Array<T> are copied finite values. List is growable through
`append(list, value)`; Array has fixed length. `length`, `get`, `updated`,
`array(list)` and `list(array)` operate without aliases or in-place mutation.
Assign a returned updated value to `let mut` to change a local variable.
`get(xs, i)` and `xs[i]` return Option<T>, None for negative/out-of-range i64.
`updated(xs,i,value)` returns Option<same collection type>. Collection and index
are evaluated once, left to right. Iteration copies elements from a snapshot.
String length, access and slicing count Unicode scalar values; `slice(s,a,b)`
returns Option<String> for 0 <= a <= b <= length. `parse_i64(s)` returns
Result<i64,String>, with stable `invalid integer` on invalid ASCII signed i64.
`concat` joins strings. No byte offsets or unchecked host indexing are exposed.

Bounds: type depth 64 and 4,096 expanded type nodes, aggregate dependency depth
64, specialization depth 32,
256 concrete function/aggregate specializations per module execution, 65,536
collection elements and 1,048,576 UTF-8 string bytes. The finite profile additionally caps each value at 65,537 total nodes and
1,048,576 total UTF-8 payload bytes. Its 1,000,000 work units charge each statement
and expression plus the result's nodes and string bytes; function call depth is
64. Legacy scalar work counters remain unchanged. The interpreter uses a bounded
16 MiB evaluation stack rather than depending on the embedding thread's stack. All bounds are checked; cyclic value types and
expanding polymorphic recursion reject. These finite copied values carry no
managed handles, loans, foreign resources or session ownership.

The data operations have these exact source signatures. The `|` notation in
this table describes supported overloads, not source union syntax.

| Operation | Signature |
| --- | --- |
| `show` | `show<T: Show>(value: T) -> String` |
| `length` | `length(List<T> | Array<T> | String) -> i64` |
| `get`, index syntax | `get(List<T> | Array<T>, i64) -> Option<T>`; `get(String, i64) -> Option<String>` |
| `updated` | `updated(List<T>, i64, T) -> Option<List<T>>`; the same form for `Array<T>` |
| `append` | `append(List<T>, T) -> List<T>` |
| `array`, `list` | `array(List<T>) -> Array<T>`; `list(Array<T>) -> List<T>` |
| `concat`, `slice` | `concat(String, String) -> String`; `slice(String, i64, i64) -> Option<String>` |
| `parse_i64` | `parse_i64(String) -> Result<i64, String>` |

## Implementation and compatibility evidence

Implementation inventory and reproducible commands follow. Runtime, foreign,
execution and command envelopes below are proposals; they earn no source
integration acceptance credit.

The smallest error-handling application is `s2b-evidence/application.cg`. It
parses strings with `parse_i64`, handles both Result cases, accumulates a List,
calls `same<T: Eq + Show>`, constructs `Box<T>`/`Choice<T>`, and handles both
checked-access cases. Its independently specified result is 42. `boundaries.cg`
records Unicode, empty/nested values, copied updates and lower/upper bounds.
`constraints.cg` executes both outcomes of the same generic Eq/Show helper at
multiple concrete types, including structural record equality and display.

| Source or target | Implemented admission in this checkpoint |
| --- | --- |
| Source check and package check | Functions, records and variants with declaration-scoped parameters; explicit/inferred application; structural Eq/Show; homogeneous copied List/Array/String operations; exhaustive matches and existing contracts. Public signatures cannot expose private nominal types. |
| Interpreter | Concrete specializations, local mutation of copied values, snapshot iteration, selected match arms, early returns, checked arithmetic/contracts, ReadArgs and all finite data operations above. Existing memory/foreign routes retain their separate admission. |
| Rust source and native debug/release | The same finite value profile specializes to concrete named functions/records/enums, checked arithmetic, explicit Option/Result and sequence wrappers. Generated Rust is standalone and uses no new dependencies. Prior scalar, native package and memory-script routes remain separate. |
| Portable Python | Finite value functions/loops/contracts, S2a variants/matches/Option/Result, concrete generics, copied immutable sequence/aggregate representations and the operations above. A finite package emits a standalone `__main__.py` archive. Legacy scalar/record-only package files and classes retain their existing representation. |
| Ordinary regions, no_gc, owner/loan aggregates and foreign/tensor effects in the finite target | Rejected before output publication. Existing specialized memory/native routes are retained; this checkpoint does not reinterpret them as portable values. |
| Notebook, verifier and advisory consumers | S2a/S2b feature admission remains conservative until S3. New syntax children are traversed; unsupported profiles are rejected rather than represented as proved or clean. The notebook worker explicitly rejects the new Sequence value form. |

`cgx2-sema/src/check.rs` and `generic.rs` check templates symbolically, including
unused templates. `nominal.rs` resolves canonical identities and substitutes
aggregate fields/cases. `foundation.rs::lower_module(module, typed)` revalidates
the unchanged summary, then specializes a bounded queue. Specialization names
are canonical callee identity plus the ordered concrete type arguments; user
record/variant names retain their arguments. A concrete call never changes its
template's effects. Package merging is internal, after original-source
visibility, checked import evidence and source-hash validation.

`FunctionSignature.type_parameters`, `Type::{Parameter,AppliedRecord,Array}`,
`TypedModule::{finite_values,bodies,calls}` and `ResolvedCall` are the concrete
shared representation additions. Per-function expression maps prevent colliding
byte spans in different modules or specializations from sharing inferred types.
`ImportedSignatures` evidence now captures both public and private variant
catalogs, in addition to all previous records/functions and transitive bodies.
Changing generic declarations/constraints or variant payload evidence invalidates
older checked-effect summaries. `verify_unchanged` still precedes publication.

Checked generic API indexes use schema 7 and include deterministic
`instantiations`: owner, canonical callee, binder-qualified type arguments,
instantiated parameters/result, and half-open UTF-8 source span. Text includes
matching `instantiate ... [checked]` rows. A generic template's symbolic
applications may still contain scoped parameters; these rows describe checked
substitution, not a claim that a runtime instance already executed. Existing
non-generic schema choices remain. Syntax-only indexes retain schema 6 and
unchecked semantic/effect/contract statuses and never invent instantiations.

Compatibility corrections: collection syntax previously parsed but failed with
CGT032; admitted literals now execute, and indexing returns Option rather than a
bare element. A mismatched `List<T>` annotation now reports a type mismatch at
runtime rather than unsupported collection syntax. List arguments can cross
checked package boundaries. Nested nominal/collection record payloads now have
finite value semantics. Fixed Array means no growth operation for an existing
value; its extent is not a const-generic type parameter. Assignment can replace
an Array variable with another Array value. No handle/loan or heterogeneous
escape hatch is introduced. Ordinary primitive arithmetic/order and core
Option/Result identity remain stable. There is no byte-index/slicing operation,
so invalid UTF-8 boundaries cannot be supplied to a string slice API.

## Proposed remaining envelopes — not implemented source APIs

The following names and shapes are proposals for coordinator review. They do
not add syntax, target admission, authentication, borrowing, foreign calls,
parallel source execution or installed commands in this delivery. Existing Rust
library APIs below remain implemented independently of these proposals.

### Ordinary function memory R03/R04

Propose `SourceMemoryV1 { function_id, signature_digest, body_digest,
source_digest, parameters: Vec<ParameterModeV1>, result: ResultModeV1,
layouts: Vec<LayoutV1>, cfg: Vec<BlockV1>, callees: Vec<CallEvidenceV1>, limits }`.
A function ID includes its module and concrete specialization. Every block,
operation, terminator, projection and call carries its original file plus a
half-open UTF-8 span. `LayoutV1 { id, nominal_identity, instantiated_arguments,
fields: Vec<(FieldId, TypeId, ProjectionId)>, ownership, storage }` authenticates
field disjointness; spelling alone never establishes nonaliasing.

`ParameterModeV1` proposes `Copy`, `Owned`, `Shared { region, loan }` and
`Exclusive { region, loan }`. `ResultModeV1` proposes `Unit`,
`Owned { from_parameter: Option<usize>, storage }` and
`Loan { from_parameter, projection, kind, region }`. Region and loan IDs have
compiler provenance and runtime storage/session identity; they are not raw
addresses. Borrow lifetimes are named source binders mapped to these identities.
A result loan must derive from an eligible borrowed input, never a local or a
callee-owned argument. Every failure, early return, unwind and cancellation edge
must include cleanup obligations and active-loan termination.

The adapter is the existing
`cgx2_ownership::v1::analyze_function(function, summaries, source, limits)` in
`crates/cgx2-ownership/ADAPTER.md`. Only `AnalysisReport::is_accepted()` admits
lowering. `CallEvidenceV1` must bind a canonical signature, body/source hash,
instantiated layouts, modes, effects/capabilities and transitive dependency
hashes. Unchecked summaries reject. Its region-bound unique-owner policy cannot
certify freely aliased managed graphs. Lowering must separately honor runtime
`Session::try_new`, `try_no_gc`, `scoped_roots` and
`ScopedRoots::{root,cleanup,finish}`: roots precede allocation safepoints and
session storage identity survives cleanup. These ordinary source modes remain
unavailable in both finite targets; managed graph and retention acceptance need
real source/runtime evidence in the next checkpoint.

### Foreign/package/tensor R04/R06/R07/R08

Propose `SourceForeignV1 { declaration_id, declaration_digest, route_id,
parameters, result, effects, capabilities, retention, release, spans }`.
`RouteIdV1` binds package/environment identity, schema-3 binding lock hash,
artifact and dependency-closure hashes, compiler/runtime pins, target triple,
ABI/symbol, and selected native/Python route. Revalidate identity before loading
or calling; an import never installs anything.

`ForeignTypeV1` distinguishes copied Unit/Bool/i64/UTF-8/bytes/homogeneous lists
and authenticated nominal layouts from `OwnedHandle { owner_token, type_id,
release_id }` and `SessionHandle { storage_id, generation, type_id }`.
`TensorSpecV1 { dtype, shape: Vec<u64>, device, layout, ownership }` binds the
actual dtype/shape/device; element-count/byte multiplication is checked before
allocation or conversion. Retention is exactly call-only or until-release with
explicit parameter indices and an owner token. Missing/uncertain release and
foreign failures preserve structured errors; raw pointers never become managed
values and handles never become copied portable aggregates.

Adapters are the existing ffi-core `ForeignValue`/`Declaration`,
`cgx2-bind/src/sdk_v2.rs`, `cgx2-ffi-native/src/sdk_v2.rs` `Sdk`/`OwnedHandle`/
`SharedHandle`, `cgx2-foreign/src/package_v1.rs`
`PackageEnvironmentV1`/`PackageAuthorizationV1`, and direct LibTorch
`Torch`/`MatrixF32`. Native tensor calls must use the pinned direct native route;
no Python fallback is permitted. Explicit Python-only calls need their reviewed
pinned environment and lifecycle. Source handles, ordinary tensor values and
retained foreign loans remain unimplemented here. Python source portability of
copied values does not claim Python-only package or tensor source integration.

### Execution R12

Propose `SourceOwnedMapV1 { operation, input_places, scalar_arguments,
checked_purity_digest, ownership_summary_digest, policy, profile_id,
cancellation_token, budgets, span }`. It can lower only proven-owned i64 buffers
to existing `owned_v1::Input::Map { values, multiplier, addend }` or
`ZipMap { left, right, addend }`. Input moves consume buffers; no managed session,
loan, retained callback or foreign handle is dispatch-safe by default.

The existing call is `owned_v1::execute(input, policy, Context {
parallel_workers, machine, profile: Option<&ValidatedProfile> },
&AtomicBool, Budgets { elapsed_ns, requested_buffer_bytes })`. Context/profile
references outlive the joined invocation. A validated profile binds operation,
machine, worker count, input size and age; Auto remains sequential without valid
benefit evidence. Propose source default `policy=auto` and a positive bounded
64 MiB requested-buffer ceiling, subject to coordinator review; the existing
library's default optional budgets remain `None`. `elapsed_ns` is observational,
not cancellation or rollback. Cancellation, overflow and allocation failures
publish no partial result. A source report must preserve selected backend,
reason/profile status, dispatch/copy/synchronization costs, requested buffer
accounting, budget status and error category. Actual source lowering and report
acceptance remain open; no performance measurement is claimed here.

### Commands R05

Propose installed routing envelopes `CommandRequestV1 { command, package_root,
target, diagnostic_format, seed, limits }` and `CommandOutcomeV1 { status,
diagnostics, dependencies, results, exit_code }` for remaining
`test`, `verify`, `notebook` and `bind` commands. Resolve and authenticate the
required engine before execution; distinguish a missing symbol from a missing
engine/runtime/solver/package. Source verification must preserve unknown and
unsupported rather than convert test success into proof.

Propose deterministic test/property discovery ordered by canonical module and
symbol identity, with explicit selection, seed default 0, bounded positive case
count default 100 and maximum 10,000, and recorded failing input/seed/span.
These defaults and discovery syntax are proposals only. Notebook routing must
retain session/outcome-unknown/no-replay contracts. Bind routing must validate
locks and versions without installation. Checked `check/build/run/emit/fmt/api`
behavior is implemented and tested; this document does not add the remaining
installed command routes.

## Reproduction and remaining gates

Use Rust 1.98.1, `CARGO_BUILD_JOBS=2`,
`CARGO_TARGET_DIR=$PWD/target/release-s2b-foundation`,
`TMPDIR=$PWD/target/tmp-s2b-foundation` (create both first), and
`LIBRARY_PATH=/tmp/cgx2-wave22-lib` where needed. No root manifest/lock or external
library changes are required; coordinator lock reconciliation is **none**.

```sh
cargo build -p cgx2-cli -p cgx2c --offline --locked
python3 crates/cgx2-syntax/s2b-evidence/run.py
cargo test --workspace --all-features --offline --locked --no-fail-fast
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The product runner uses a fresh unrelated working directory, actually executes
both native profiles and emitted Python, compiles/executes standalone emitted
Rust, checks deterministic API metadata and verifies preservation of previous
artifacts on rejected source. `report.json` records those outcomes, fixture
hashes and an implementation-tree hash. `integration.json` records workspace
counts and command outcomes. The two long bounded probes would exhaust the work
budget if the tested scrutinee or index were evaluated twice.

Local HTTP and child-process lifecycle integration fixtures require their normal
local execution permissions; the initial restricted run reported permission
failures and was rerun with those permissions. No host security setting changed.

All remaining ordinary memory, foreign/package/tensor, execution and command
source integrations above remain open inside S2b. Full admitted Rust support,
S3 notebook/verifier/analyzer/foreign adapters, the R10 blinded readability study,
leak-enabled safety acceptance, real solver/isolation/runtime measurements and
R13–R15 external model/human/release gates remain separate and unclosed. Grammar
is still a candidate pending R14. There is no release or S2b-completion claim.

Final verification for this delivery: **1,092 passed, 0 failed, 6 ignored** in
243 workspace suites; **159/159 product checks** passed. All-feature/all-target
Clippy with warnings denied, default feature build, formatting and diff checks
passed. See `s2b-evidence/integration.json` for exact commands and provenance.
