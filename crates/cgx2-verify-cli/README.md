# `cgx2-verify`: external JSON contract checker

This command checks an external JSON v1 contract. `.cg` source integration belongs to a separate track. The finite engine checks the explicit integer domains and both Boolean values; it does not establish a theorem over all signed `i64` inputs.

```sh
RUSTUP_TOOLCHAIN=1.98.1 cargo run -p cgx2-verify-cli --offline -- crates/cgx2-verify-cli/examples/valid.json --format json
```

```text
cgx2-verify CONTRACT.json [--format text|json] [--max-cases N] [--solver PATH] [--timeout-ms N]
```

`--format` defaults to `text`. `--max-cases` defaults to 100,000 and must be 1–1,000,000. `--timeout-ms` defaults to 2,000 and must be 1–10,000; it applies separately to the version, condition, and witness subprocesses. Input is limited to 65,536 bytes, 32 parameters, 1,024 expression nodes, and depth 64. Solver stdout and stderr are each retained up to 16,384 bytes. Invalid input exits 2; any valid contract result exits 0.

The input's top-level keys are `version`, `parameters`, `domains`, `requires`, and `ensures`. `version` is `1`. A parameter has `name` and `sort` (`i64` or `bool`). Each `i64` parameter has exactly one inclusive `{"name":"x","min":-2,"max":3}` domain; Boolean domains are implicit. Duplicate, missing, or extra domains are errors. Expressions use `op` values `bool`, `i64`, `var`, `add`, `sub`, `eq`, `lt`, `le`, `gt`, `ge`, `and`, `or`, `not`, or `call`, with the usual `value`, `name`, `left`/`right`, `arg`, or `args` fields. `call` is an unsupported foreign result for executable verification.

## Result JSON v2

`schema_version` is `2`; the input contract version remains `1`. `input_sha256` hashes exact input bytes. `verdict` is the finite enumeration result: `proved` means **finite domain only**, `disproved` includes a re-evaluated finite counterexample, and `unknown` includes a reason. `proof` is the global result. It is `disproved` only with a validated counterexample from finite enumeration or a solver model; otherwise it is `unknown` with `no_trusted_solver_proof`. This version never emits a global `proved` result.

With `--solver PATH`, the command runs `PATH --version`, then sends a four condition SMT-LIB script to `PATH` through stdin. The four statuses correspond to precondition definedness, satisfiable precondition, false postcondition, and postcondition overflow, in that order. The `solver` block records the supplied path, reported version, script SHA-256, statuses, bounded output, exit and timeout details. Its `trust` is always `untrusted_protocol`. A complete `unsat/sat/unsat/unsat` transcript has response `unsat_protocol_only` and leaves `proof` unknown. A SAT violation triggers a separate `get-value` query; `witness_attempts` records its script hash, bounded output, exit, and validation. Only a complete model that violates the original contract under checked `i64` evaluation yields `validated_counterexample` and global `disproved`. Bad models, fake UNSAT, unknown statuses, missing solver, crashes, floods, and timeouts never establish proof.

No installed, version-pinned real solver is trusted by this implementation. The live proof gate is skipped where Z3/CVC5 are unavailable. The solver path and `--version` text are observations, not attestation. Uninterpreted calls have pure, total, bounded-result assumptions in SMT; they are never validated as counterexamples against external code.
