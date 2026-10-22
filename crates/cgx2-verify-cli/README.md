# `cgx2-verify`: finite contract checker

This command reads an external JSON contract, not a `.cg` source file. It enumerates an explicit finite input scope using `cgx2-verify-core`. A `proved` verdict means every listed input was checked and at least one satisfied `requires`; it is not a global theorem. A false `ensures` or checked postcondition overflow yields a reevaluated counterexample. Inconsistent preconditions, exhausted case limits, and uninterpreted calls yield `unknown`.

Run offline from the workspace root:

```sh
cargo run -p cgx2-verify-cli --offline -- crates/cgx2-verify-cli/examples/valid.json --format json
cargo run -p cgx2-verify-cli --offline -- crates/cgx2-verify-cli/examples/false.json
cargo run -p cgx2-verify-cli --offline -- crates/cgx2-verify-cli/examples/inconsistent.json
cargo run -p cgx2-verify-cli --offline -- crates/cgx2-verify-cli/examples/unsupported.json
```

The executable is named `cgx2-verify`. Usage:

```text
cgx2-verify CONTRACT.json [--format text|json] [--max-cases N] [--solver PATH] [--timeout-ms N]
```

`--format` defaults to `text`. `--max-cases` defaults to 100,000 and must be 1–1,000,000. `--timeout-ms` defaults to 2,000 and must be 1–10,000; it applies separately to the optional solver version and query processes. Input is at most 65,536 bytes; at most 32 parameters, 1,024 expression nodes, and expression depth 64 are accepted. Each solver output stream retains at most 16,384 bytes. Invalid input/arguments exit 2; a valid contract with any verdict exits 0.

## Version 1 JSON

The top-level object has exactly `version`, `parameters`, `domains`, `requires`, and `ensures`. `version` is the integer `1`. Parameters are `{"name":"x","sort":"i64"}` or `{"name":"b","sort":"bool"}`, in evaluation order. Every `i64` parameter has exactly one explicit inclusive `{"name":"x","min":-2,"max":3}` domain. Boolean parameters range implicitly over false and true and have no domain entry. Names are nonempty strings of at most 128 UTF-8 bytes; duplicate parameter/domain names, missing/extra domains, empty ranges, fractional numbers, unsupported fields and ill-typed contracts are rejected.

Expressions use tagged objects with an `op` field:

| `op` | Other fields | Result |
| --- | --- | --- |
| `bool` | `value`: Boolean | Boolean literal |
| `i64` | `value`: signed i64 integer | Integer literal |
| `var` | `name`: parameter name | Parameter value |
| `add`, `sub` | `left`, `right`: expressions | Checked i64 addition/subtraction |
| `eq` | `left`, `right`: same-sort expressions | Boolean equality |
| `lt`, `le`, `gt`, `ge` | `left`, `right`: i64 expressions | Boolean comparison |
| `and`, `or` | `left`, `right`: Boolean expressions | Boolean operation; both sides are evaluated |
| `not` | `arg`: Boolean expression | Boolean negation |
| `call` | `name`, `sort` (`bool`/`i64`), `args`: expression array | Explicit uninterpreted call; bounded result is `unknown` |

`requires` and `ensures` must both have Boolean sort. The four example files show a finite proof, a false contract, an inconsistent precondition, and an unsupported foreign call. JSON output has `schema_version: 1`, a SHA-256 of the exact input bytes, and a bounded `verdict` with input scope. The same bytes and options produce byte-identical output without `--solver`.

## Optional solver observation

`--solver PATH` runs an executable that accepts SMT-LIB from stdin without additional arguments. The command first tries `PATH --version`, then sends the emitted three-query script to `PATH`. JSON records the supplied path, version (or `unavailable`), script SHA-256, protocol status lines, bounded stdout/stderr, exit/timeout, and assumptions. The solver block always says `trust: "untrusted_protocol"`. Even `unsat/sat/unsat` is only `unsat_protocol_only`, not a global `Verdict::Proved`. A `sat` result is only an unvalidated candidate until a model is parsed and reevaluated. A missing solver leaves the bounded verdict intact and reports `unknown:SolverUnavailable` in the solver block.

SMT uses mathematical `Int` with signed i64 parameter bounds and range guards for checked addition/subtraction. Explicit uninterpreted functions are assumed pure and total; their i64 results are bounded. These assumptions differ from a complete model of arbitrary foreign code. No source contract integration, trusted proof/model checker, or live SMT gate is provided by this command.
