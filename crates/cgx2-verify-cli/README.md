# `cgx2-verify`

The command checks a JSON contract or one checked `.cg` function. The finite
engine enumerates every supplied `i64` value and both values of each `Bool`
parameter. Its `verdict.kind: proved` covers only the listed finite domain.
The separate `proof.kind` describes the global checked fragment.

```text
cgx2-verify CONTRACT.json [--format text|json] [--max-cases N] [--solver PATH] [--timeout-ms N]
cgx2-verify FILE.cg [--function NAME] [--domain NAME:MIN:MAX]... [--format text|json] [--max-cases N] [--solver PATH] [--timeout-ms N]
```

`--format` defaults to `text`; `--max-cases` defaults to 100,000 and accepts
1–1,000,000. Each `i64` parameter needs one inclusive `--domain`; Boolean
parameters need none. `--timeout-ms` defaults to 2,000 and accepts 1–10,000.
Input is capped at 65,536 bytes. Invalid input exits 2; a valid result exits 0.

## Checked source fragment

The selected function must be public, semantically checked, declare
`effects()`, have `i64`/`Bool` parameters and return type, exactly one
`requires`, exactly one `ensures`, and one pure expression body or return.
The source adapter accepts literals, parameters, `result` in `ensures`, checked
`+`/`-`, and `==`, `!=`, `<`, `<=`, `>`, `>=`. The current source parser does not
expose Boolean `and`/`or`/`not` syntax, although typed JSON/core contracts
support those operators. Calls, `assert` statements, multiplication/division,
loops, foreign declarations, and effects return `unknown` with a zero-based,
half-open source byte span. Source results use `schema_version: 3`; JSON
contract results use `schema_version: 2`.

## Encoding audit and solver trust

Each result includes generated `smtlib` and a `solver_fixture_catalog` with
true, false, and overflow contracts, expected four-status transcripts, and
script hashes. `bounded_solver_result` says whether an independent SMT-term
interpreter agreed with the contract evaluator on **every tuple** in the
requested finite domain. For `.cg`, it also compares the parsed source AST.
The four named obligations are `requires_defined`, `requires_satisfiable`,
`ensures_false`, and `ensures_overflow`; the last includes substituted body
overflow. A failed or unrun audit cannot support a global proof.
Saved copies and expected statuses are in
[`examples/proof-encoding-audit/manifest.json`](examples/proof-encoding-audit/manifest.json).
Its `live_solver: unrun` records the absence of Z3/CVC5 on the audit host.

`--solver PATH` accepts an executable named exactly `z3` or `cvc5` for the
global proof gate. The result records its canonical path, SHA-256, version,
arguments, four complete status lines, bounded output, exits, and fixture
probes. Global `proved` requires checked `.cg` source, successful source/SMT
parity, all three live fixture probes, stable recognized solver identity, and
exactly `unsat/sat/unsat/unsat` for the four obligations. This is a conditional
solver-backed claim for the documented pure fragment. A path, hash, and
version string alone do not attest a binary. With no live solver,
`proof.kind` is `unknown` with reason `live_solver_unrun` after source audit.

A finite counterexample becomes `proof.kind: disproved` only after the audit
cross-checks it. Solver SAT is only a candidate: a separate `get-value` query
must supply a complete in-range model violating checked semantics and, for
`.cg`, the source evaluator. Unrecognized stub solvers exercise transport and
protocol rejection only. Unknown, malformed, truncated, inconsistent, or
over-limit output never proves. Each process has its own deadline; stdout and
stderr are each capped at 16 KiB; Linux solver processes have a 512 MiB
address-space limit.

```sh
RUSTUP_TOOLCHAIN=1.98.1 cargo run --offline -p cgx2-verify-cli -- crates/cgx2-verify-cli/examples/finite.cg --domain x:0:2 --format json
```
