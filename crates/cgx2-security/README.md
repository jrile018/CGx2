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

## Checked package advisory analysis (release S1-10)

```sh
cargo run --offline -p cgx2-security -- audit --package crates/cgx2-security/examples/package-audit --policy crates/cgx2-security/examples/package-audit/policy.json --format sarif
```

The example exits 1 for advisory findings. Its foreign boundary is the harmless
`builtins.str`; a taint boundary finding identifies data flow, not an injection
vulnerability. No function is executed and no Python/native library or binding lock
is loaded. A schema 2 manifest's lock path is metadata for this analysis, not a
verified foreign installation. Exit codes remain 0 (no findings), 1 (findings),
2 (bad policy/package/provenance).

The additive `package_v1` API exports `audit_package(root, policy, limits)` and
`audit_checked_package(&CheckedPackage, policy, limits)`. The former loads a checked
package; both use the effect index's exact checked public edges and typed
body-derived effects/capabilities. There is no unchecked-package fallback or caller
provided imported-effect trust. `PackagePolicy { effects: Policy,
allowed_capabilities }` has a separate parser, `parse_package_policy`, for:

```json
{"schema_version":2,"allowed_effects":["ReadArgs","Foreign"],"allowed_capabilities":["ArgsRead","ForeignCall"],"allow_unknown":false}
```

Capability labels must be nonempty ASCII alphanumeric/underscore strings. Duplicate
labels and extra fields reject. Existing schema 1 policies/single-file APIs keep
their behavior. Capability policy checks only produce advisory findings; they do
not admit execution, create capability tokens or configure confinement.

`AnalysisLimits::default()` permits 16,384 AST steps **across the entire package**
and 16 call frames. Library callers may lower either positive bound; larger or zero
limits reject. The CLI uses defaults. Recursive, over-depth and exhausted paths
report unknown. Even if `allow_unknown` is true for the effect policy, taint unknown
findings remain visible. Every function is also analyzed with unknown public input
parameters; findings from those entry analyses can coexist with a precise clean or
tainted invocation of that function.

| Rule | Checked package support | Unknowns and limits |
| --- | --- | --- |
| CGSEC001 | Typed direct/transitive effects checked against the effect allowlist at declarations and resolved call sites; related callee declarations cross module boundaries. | Checked-loader errors reject input. Builtin direct effects are located at the containing declaration/script entry. |
| CGSEC002 | Admitted unresolved call/effect summaries report unknown when policy disallows unknown. | Most unresolved/computed/shadowed source calls are already rejected by the frozen checker; those return input errors rather than fabricated checked edges. |
| CGSEC003 | Explicit `arg(...)` data reaches any checked foreign declaration through positional scalar arguments, bindings, assignments, binary expressions and bounded direct local/public imported calls; cross-file source origin and foreign declaration locations accompany the sink. | This is a conservative foreign-boundary rule, not a per-library vulnerability model. No sanitizers or foreign implementations are trusted. |
| CGSEC004 | Both branches are visited; outer bindings join while branch-local shadows restore. Loops widen outer values before/after a bounded body visit and retain possible returned taint. | Every loop is unknown, including zero iterations; recursive/deep calls, step exhaustion, memory alias/state behavior, qualified assignments, unsupported argument mapping and external return taint are unknown. Effects in unsupported loop bodies are checked input errors. |
| CGSEC005 | Typed direct/transitive capabilities checked against the capability allowlist at declarations and resolved call sites. | No runtime capability enforcement or validation of an external library's honesty. |

Clean literals stay clean through admitted pure positional forwarding. The model
joins possible return/tail-expression flows conservatively and may flag paths
that cannot execute. It does not model implicit information flow, sanitizer
semantics, heap aliases, general dynamic dispatch or foreign internals. External
return values remain unknown even if every argument was clean. A zero-finding
result establishes no general vulnerability-absence claim.

Package JSON uses schema 2 and analysis tag `checked_package_security_v1`;
`PackageReport::findings()` returns relative locations with zero-based half-open
UTF-8 byte spans and related origins/declarations. SARIF 2.1.0 uses percent-encoded
package-relative URIs and one-based Unicode code point columns. Results sort by
source/span/rule/message/related locations. JSON records manifest/source hashes and
limits. `to_json`, `to_text` and `to_sarif` return `Result` and recheck all checked
source/manifest provenance before publishing. Neither format includes source text
or absolute package roots. Snapshot accessors require `analysis().verify_unchanged()`
before a caller publishes a custom representation.

These are **source-aware advisory library and standalone analysis commands** over
the existing checked `.cg` profile. They do not add source language constructs,
modify the compiler/installed `cgx2` command, run foreign code, or close empirical
security acceptance. See [EVIDENCE.md](EVIDENCE.md) for verification, exact remaining
Stage 3 adaptation and external handoffs.
