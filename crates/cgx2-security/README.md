# CGx2 security policy preview

`cgx2-security` audits one UTF-8 `.cg` file with a versioned external policy. It is an advisory syntax analysis. Exit status is `0` for no findings, `1` for findings, and `2` for invalid policy, input, or parse failure.

```sh
cargo run -p cgx2-security --offline -- audit demo.cg --policy policy.json --format sarif
```

`--format` accepts `text` (default), `json`, or `sarif`. JSON has schema version 1 and zero-based, half-open UTF-8 byte spans. SARIF uses version 2.1.0 and one-based Unicode code point line/column positions; its file path is URI encoded. Results sort by span, rule, message, then related span. Neither machine format embeds source text; SARIF includes the path passed to the command.

Example `policy.json`:

```json
{"schema_version":1,"allowed_effects":["ReadArgs","ManagedAlloc"],"allow_unknown":false}
```

`allowed_effects` may name `Collect`, `Foreign`, `ManagedAlloc`, `Mutate`, and `ReadArgs`. It is required, but may be empty. `allow_unknown` defaults to `false`; when false, unknown and imported calls receive `CGSEC002`. The analyzer rejects extra fields, invalid effect names, duplicates, and schema versions other than 1. The source limit is 1,048,576 bytes and policy limit is 65,536 bytes. Taint walks at most 16,384 AST steps and 16 local call frames, reporting `CGSEC004` when the bound is reached.

| Rule | Meaning |
| --- | --- |
| `CGSEC001` | Direct or transitive effect is absent from `allowed_effects`; assignments count as `Mutate`. |
| `CGSEC002` | Call effects cannot be established, including imports, foreign execution, shadowed/computed calls, and unsupported calls. |
| `CGSEC003` | A value from `arg(...)` reaches an argument of exact `python.call` or `ffi.call`. The finding has the sink span and related source span. |
| `CGSEC004` | Taint flow is unknown at a sink or at an unsupported alias, loop, or local call bound. |

The taint model follows direct local immutable/mutable bindings, assignments, scalar binary expressions, and same-file direct calls with matching parameter counts. It does not recognize sanitizers by name. A clean literal at a sink does not produce a taint finding. `arg_count` is not a taint source. Seeded inputs live in `tests/fixtures/`. This analyzer uses the effect index in explicit syntax-only mode because foreign calls and memory operations inside functions are outside the frozen semantic checker. It does not claim that source passed a type check.

| Seeded case | Expected coverage or limit |
| --- | --- |
| `tainted_binding.cg` | Detected as `CGSEC003` with source and sink spans. |
| `local_forward.cg` | Detected through a direct local call. |
| `clean_literal.cg` | No taint finding; policy may still reject `Foreign` or `Unknown`. |
| `unknown_alias.cg` | Unknown taint at the sink, not claimed safe. |
| A local function with a parameter at a sink, called only with a literal | `CGSEC004` may be a false positive because each function is also audited with unknown parameter values. |
| Qualified alias assignment or loop-carried flow | `CGSEC004`; the analyzer does not prove the later value safe or tainted. |
| Dynamic dispatch, Python/PyTorch internals, cross-file body, or sanitizer behavior | Missed concrete exploit paths remain possible; only the documented calls and direct flow are inspected. |

Allowed effects are policy labels, not runtime capability tokens. A zero-finding result does not establish general injection safety, enforce source capabilities, or clear the first-release security gate.

## Bounded Boolean conditional profile (Wave 24)

Conditional analysis visits the guard and both branches regardless of literal
guards. Outer taint states join conservatively; branch-local bindings are restored
or removed at scope exit. Conditional return taint is retained across later
fallthrough statements/returns. This remains an advisory bounded explicit-flow
analysis, not a proof of all paths or implicit information flows.
