# CGx2 public API index (Wave 3)

This library indexes one checked `cgx2_syntax::Module`. Call
`cgx2_sema::check_module(&module)` first, then
`cgx2_api_index::build_index(&module, &typed)`. It returns an `ApiIndex` with
`schema_version: 1` for legacy source or `2` when declarations/contracts are present, an optional dot-joined module name, and public source
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

## Schema versions

`render_json()` emits one compact UTF-8 JSON object, without a trailing newline:

```json
{"schema_version":2,"module":"app.math","functions":[{"name":"double","parameters":[{"name":"x","ty":"i64"}],"return_type":"i64","span":{"start":17,"end":59},"effects":[],"effects_status":"inferred","capabilities":[],"capabilities_status":"inferred","requires":[],"ensures":[]}]}
```

Modules without any effect, capability, or contract annotations keep the exact version 1 JSON and text rendering. Version 2 adds the fields shown above. The `ApiFunction` Rust object exposes `inferred` status even for a legacy module, while its version 1 JSON keeps the old shape. `module` is `null` if undeclared; `functions` is `[]` if no public functions
exist. A span is a zero-based, half-open UTF-8 byte range in the source passed
to the parser. The renderer escapes JSON control characters, quotes, and
backslashes; other Unicode text remains UTF-8. `render_text()` starts with
`module NAME` (or `module <unnamed>`), then writes one sorted signature per
line as `pub fn NAME(PARAM: TYPE) -> TYPE effects(...) [STATUS] caps(...) [STATUS] [START, END)`, followed by contract summaries labeled `runtime-checked`.

Explicit effect/capability lists retain declared names; omitted lists contain conservative inferred names and use status `inferred`. Each contract has canonical expression text, a source span, and status `runtime-checked`. No contract is labeled proved. This is a per-module library; documentation comments are not indexed, and a `cgx2 api` CLI command requires later integration.
