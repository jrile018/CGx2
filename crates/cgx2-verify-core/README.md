# CGx2 bounded verification core

This crate accepts typed external `Contract` values. It does not parse `.cg` source. The supported executable subset is Boolean expressions and checked signed `i64` addition/subtraction, equality, and ordering. Both operands of `and` and `or` are evaluated. An uninterpreted call is outside this subset.

`verify_bounded(&contract, &domains, max_cases)` enumerates inclusive integer domains and both Boolean values. `Verdict::Proved` covers **only** the listed finite inputs, with at least one input satisfying `requires`. A false `ensures` or checked postcondition overflow returns a counterexample re-evaluated against the original contract. Inconsistent or overflowing preconditions, unsupported calls, and an exhausted case cap return `Unknown`.

`emit_smtlib` uses mathematical `Int` constrained to signed `i64` parameters and explicit checked arithmetic guards. Its four independent `check-sat` conditions, in transcript order, are:

| Index | Label | SAT means |
| --- | --- | --- |
| 0 | `requires_defined` | Some input makes precondition arithmetic undefined; proof is unknown. |
| 1 | `requires_satisfiable` | At least one defined input satisfies the precondition. UNSAT is vacuous, reported unknown. |
| 2 | `ensures_false` | A satisfying input makes a defined postcondition false. |
| 3 | `ensures_overflow` | A satisfying input makes postcondition arithmetic undefined. |

`parse_solver_result` accepts exactly four status lines, zero exit code, and empty stderr. `unsat, sat, unsat, unsat` is only `SolverResponse::Unsat`: an **untrusted protocol observation**, never a formal proof. An `unknown`, malformed output, timeout, or inconsistent precondition stays unknown. SAT on either postcondition condition is only a candidate.

`emit_witness_smtlib(contract, failure)` reissues one violation condition and requests all parameter values. `validate_solver_witness` accepts one complete `sat` plus `get-value` response. It checks variable names, sorts, duplicates, missing assignments, trailing output, and signed `i64` bounds, then evaluates `requires` and `ensures` using checked arithmetic. Only a real violation becomes a validated `Counterexample`. An arbitrary executable can supply such a witness, but its UNSAT claim never becomes a proof. Uninterpreted calls cannot produce a validated witness because their behavior cannot be re-evaluated.

Solver observation retains at most 16 KiB per output stream, includes stdin write and pipe closure in its deadline, and kills the process group on Unix at timeout. No version-pinned solver trust path is configured. When Z3/CVC5 are absent, tests check the generated script structure and adversarial fake protocols; live solver proof coverage is skipped.
