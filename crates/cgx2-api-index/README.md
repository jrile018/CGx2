# CGx2 public API index (Wave 3)

This library indexes one checked `cgx2_syntax::Module`. Call
`cgx2_sema::check_module(&module)` first, then
`cgx2_api_index::build_index(&module, &typed)`. It returns an `ApiIndex` with
`schema_version: 1` for legacy source, `2` when declarations/contracts are present, or `3` when a foreign declaration exists, an optional dot-joined module name, and public source
functions sorted by name and declaration byte span. `find(name)` matches an
exact name and returns `None` for a missing public function. A missing module
declaration is represented by `module: None`; it is distinct from a missing
function. Private functions and semantic builtins are excluded.

`build_index` rechecks the supplied AST and compares the complete resulting
`TypedModule` with the supplied summary. Failed checking returns
`IndexError::InvalidModule` with the semantic diagnostics; a differing summary
returns `IndexError::MismatchedTypedModule`. The current semantic summary has no
AST identity or source digest, so two distinct ASTs that produce identical
typed summaries cannot be distinguished. Source edits that change only a
literal or comment can fall in this category. The caller should retain the AST
and typed summary as a pair from the same checking run.

For an imported package module, use `build_index_with_imports(&module, &typed,
&environment)`, passing the same `ImportedSignatures` used by
`check_module_with_imports`. It rechecks against that environment and keeps the
per-module API schema at version 1 or 2. Package graph fields belong to
`cgx2-package` schema version 2.

A public `foreign python pub fn` entry in schema 3 carries `foreign.route`, `foreign.target`, `foreign.ownership`, and `foreign.runtime` alongside its scalar signature, `Foreign` effect, and `ForeignCall` capability. The route is Python-only and `runtime: "local:cpython"` records the linked local runtime identity; it is not a package-version pin. An ordinary public function has `foreign: null` in schema 3. See the runnable example in `cgx2-cli/README.md`.

## Schema versions

`render_json()` emits one compact UTF-8 JSON object, without a trailing newline:

```json
{"schema_version":2,"module":"app.math","functions":[{"name":"double","parameters":[{"name":"x","ty":"i64"}],"return_type":"i64","span":{"start":17,"end":59},"effects":[],"effects_status":"inferred","capabilities":[],"capabilities_status":"inferred","requires":[],"ensures":[]}]}
```

Modules without any effect, capability, or contract annotations keep the exact version 1 JSON rendering. Version 2 adds the fields shown above. The `ApiFunction` Rust object exposes `inferred` status even for a legacy module, while its version 1 JSON keeps the old shape. `module` is `null` if undeclared; `functions` is `[]` if no public functions
exist. A span is a zero-based, half-open UTF-8 byte range in the source passed
to the parser. The renderer escapes JSON control characters, quotes, and
backslashes; other Unicode text remains UTF-8. `render_text()` starts with
`module NAME [checked]` (or `module <unnamed> [checked]`), then writes one sorted signature per
line as `pub fn NAME(PARAM: TYPE) -> TYPE effects(...) [STATUS] caps(...) [STATUS] [START, END)`, followed by contract summaries labeled with their stored status.

Explicit effect/capability lists retain declared names; omitted lists contain conservative inferred names and use status `inferred`. Contracts carry canonical expression text, source span, and `runtime-checked` status; none is labeled proved. `cgx2 api FILE.cg --json` renders this checked per-module index. Documentation comments are not indexed.


A checked public memory function exposes its declared `i64` scalar parameters, `Unit` result, and explicit or transitive effects and capabilities in schema 2. Memory handles cannot cross call boundaries. For example, `pub fn allocate(size: i64) -> Unit effects(ManagedAlloc)` has `{"name":"size","ty":"i64"}` in `parameters`.

Checked object-reference operations stay within local memory functions. The index records `ManagedAlloc`, `Mutate`, and `Collect` effects and `MemoryCollect` capability when required; it never exposes object handles as API parameters or results. `build_index` rechecks bounded object shape and same-region rules, returning `IndexError::InvalidModule` for invalid source.

## Human output and syntax-only indexing

Human output always includes the stored `semantics_status` in the module header.
Checked indexes use `[checked]`; `build_syntax_index` uses `[syntax-only]`, with
unchecked effect, capability, and function-contract metadata. Checked contracts
retain `runtime-checked`; this describes runtime enforcement, not execution of
the indexed function or a proof. Loop annotations retain their own stored
status (currently `unchecked`, even in a checked index) and half-open byte spans.

Schema 6 retains nominal record/variant identities, recursive applied types,
generic type parameters and named constraints, loop annotations, and semantic
status. Human declarations use qualified aggregate identities and complete
types such as `cgx2.core.Option<cgx2.core.Result<i64, Bool>>`. Type parameters
render as `<T: Eq + Show, U>`; parameter/field names and source declaration order
are preserved. Only public declarations enter the index, sorted deterministically
by the existing builders. An unannotated result retains the model's result
string (`<inferred>` in syntax-only mode). Foreign declarations show their stored
unsafe flag independently of optional checked binding metadata.

These human-format changes do not alter JSON bytes, schema selection, public
model shapes, or semantic admission. Older JSON versions continue to omit
fields outside their schema. Golden JSON fixtures in `tests/fixtures/text-*.json`
were captured from the repair baseline before changing the text renderer.
