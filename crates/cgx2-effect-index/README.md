# CGx2 call and effect index

This standalone advisory tool indexes one `.cg` module. It does not enforce effects, prove purity, or authorize parallel or GPU execution.

```text
cgx2-effect-index FILE.cg [--format text|json] [--syntax-only]
```

`text` is the default. Checked mode parses and runs `cgx2-sema::check_module`; parse and semantic failures exit nonzero with a code, stage, message, and location. Use `--syntax-only` explicitly for unsupported source, including memory operations inside functions and unresolved calls. Syntax-only results say `"check_mode":"syntax_only"`; unchecked names, computed callees, and shadowed targets yield `Unknown`. The tool reads at most 1,048,576 bytes and accepts UTF-8 only. There is no implicit fallback after a failed check.

## JSON schema version 1

The output is one JSON object followed by a newline. Fields appear in this order: `schema_version` (integer `1`), `span_unit` (`"utf8_byte_offset"`), `module` (dotted name or `null`), `check_mode` (`"checked"` or `"syntax_only"`), `functions` (array), `top_level` (entry), and `cycles` (array of name arrays). Every span is a zero-based, half-open UTF-8 byte range in the input file: `{"start":N,"end":N}`. A missing top-level statement set has span `[0,0)`.

Each entry has `name`, `span`, `public`, `parameters` (`{"name","type"}` array), `return_type` (text or `null`), `calls`, `direct_effects`, `transitive_effects`, and `recursive`. Calls have `target` (dotted text or `null` for computed callees), `kind` (`local`, `builtin`, `foreign`, `unknown`), and call-expression `span`. Types come from source annotations; checked unannotated returns use the semantic checker inference. The top-level entry is named `<top-level>`. Functions sort by name, calls by call-site start/end then target, effects by the vocabulary order below, and cycles and members lexicographically. The graph includes calls in arguments and computed callee expressions.

Only same-module named functions are resolved. A parameter, local binding, or imported namespace with the same root name makes a target unknown. Calls to other modules, computed callees, and every unlisted builtin yield `Unknown`; the index never infers dynamic dispatch or import resolution. In syntax-only mode, unbound value names yield `Unknown`; exact memory calls inside functions also include `Unknown` because the frozen checker does not model them there. Recursion appears as `recursive:true` and in `cycles` (strongly connected groups, including self-calls). Effects propagate from callees to callers and top level until no set changes.

| Exact call or syntax | Direct effect |
| --- | --- |
| `arg`, `arg_count` | `ReadArgs` |
| `memory.region`, `memory.owned_bytes`, `memory.alloc_bytes`, `memory.copy_to_owned` | `ManagedAlloc` (CGx2 allocation, including owned buffers and region setup) |
| `memory.promote_owned` | `ManagedAlloc`, `Mutate` (allocates managed storage and consumes the owned value) |
| `memory.collect` | `Collect` |
| `memory.move`, `memory.borrow_mut`, `memory.end_borrow`, `memory.drop`, `memory.close` | `Mutate` |
| `memory.borrow`, `memory.use` | empty |
| `ffi.call`, `python.call`, `python.import`, `torch.call` | `Foreign`, `Unknown` (syntax-only modeling names, not a supported source bridge) |
| assignment (`=` or `+=`) | `Mutate` |
| every other call name or computed callee | `Unknown` |

`NoGc`, local immutable binding, and a read-only scalar expression add no direct effect; their nested calls still count. Effect vocabulary and output order: `Collect`, `Foreign`, `ManagedAlloc`, `Mutate`, `ReadArgs`, `Unknown`. An empty set means only that this bounded analysis found no effect. Memory intrinsics are recognized by exact spelling, even in syntax-only mode; no new memory syntax is introduced here.

Example `demo.cg`:

```cg
fn double(x: i64) -> i64:
    x * 2
```

`cgx2-effect-index demo.cg --format json` produces (spans include the final newline):

```json
{"schema_version":1,"span_unit":"utf8_byte_offset","module":null,"check_mode":"checked","functions":[{"name":"double","span":{"start":0,"end":36},"public":false,"parameters":[{"name":"x","type":"i64"}],"return_type":"i64","calls":[],"direct_effects":[],"transitive_effects":[],"recursive":false}],"top_level":{"name":"<top-level>","span":{"start":0,"end":0},"public":false,"parameters":[],"return_type":null,"calls":[],"direct_effects":[],"transitive_effects":[],"recursive":false},"cycles":[]}
```

The corresponding text view lists each entry, call site, direct effects, and propagated effects. Current syntax disallows mixing function items and top-level script statements in one module; use a script-only file to index top-level calls. Checked mode is limited to the current scalar checker and frozen top-level memory script profile. Syntax-only mode is an analysis aid and must never be treated as a successful semantic check.

## Bounded Boolean conditional profile (Wave 24)

Conditional guards and both branches contribute call sites and transitive
function effects/capabilities. Branch binding sets are scoped independently;
a literal guard does not remove an inactive branch from the static index.

## Checked package API and command (release S1-10)

```sh
cargo run --offline -p cgx2-effect-index -- --package crates/cgx2-security/examples/package-audit --format json
```

`package_v1::index_package(root)` calls `cgx2_package::load_checked_package`;
`index_checked_package(&CheckedPackage)` accepts an existing immutable checked
snapshot. Neither accepts caller-invented imported signatures or trusted effects.
`PackageAnalysis::entries()` exposes manifest-relative `Identity { source, name }`,
declarations, resolved calls, body-derived effects and capabilities. Function
summaries come directly from `CheckedModule::typed().effects`; explicit annotations
are upper bounds, so an effectful annotation on a pure body does not invent effects.
Each imported function edge must match the loader's exact public resolved name and
call span. Private, unimported, duplicate, shadowed or computed invalid source is
rejected by the checked loader rather than downgraded to successful analysis.
Unknown admitted calls remain `Resolution::Unknown` and add `Unknown`.

The versioned JSON analysis tag is `checked_package_effects_v1` (schema 1, distinct
from the single-file schema). It includes relative source paths, UTF-8 byte spans,
source SHA-256 values and the manifest SHA-256. Entries sort by source/name; calls
sort by span/name; effects and capabilities sort lexically. Function identities
remain distinct even when multiple modules export the same short name. Checked
range/comparison intrinsics and local/imported record constructors are pure in the
current profile. Both conditional branches and loop bodies contribute static
summaries even when inactive. Loops that violate the frozen checker's purity
restriction remain input errors. Foreign declarations contribute their checked
`Foreign` effect and `native`/`ForeignCall` capability without opening libraries.

`verify_unchanged`, `to_json` and `to_text` revalidate manifest/source bytes and
canonical paths. Serializers return `Result`; a drifted snapshot cannot publish
fresh output. Data read directly through accessors describes the captured snapshot
and callers must verify provenance before publishing it. `--package` rejects
`--syntax-only`; existing single-file interfaces and JSON remain unchanged.

The owned S1-10 delivery evidence and Stage 3 handoff are in
[`../cgx2-security/EVIDENCE.md`](../cgx2-security/EVIDENCE.md).
