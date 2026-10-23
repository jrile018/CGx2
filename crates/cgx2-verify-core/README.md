# CGx2 verification core

`Contract` represents a pure Boolean and checked signed-`i64` fragment.
Supported expressions are literals, variables, `+`, `-`, equality, ordering,
`and`, `or`, and `not`; both Boolean operands evaluate. `Expr::If(guard, then, else)`
requires a Boolean guard and equal branch sorts. It evaluates the guard once
and only the selected branch; validation and uninterpreted-call detection
still inspect all three children. Uninterpreted calls
are outside the executable and trusted proof fragment.

`verify_bounded(contract, domains, max_cases)` enumerates inclusive integer
domains and both Boolean values. `Verdict::Proved` covers only those inputs.
False postconditions and checked overflow return a finite counterexample;
undefined/inconsistent preconditions, unsupported calls, or a case cap return
`Unknown`.

`emit_smtlib` models integers as mathematical `Int` values constrained to
signed `i64`, with explicit range guards for checked `+`/`-`. It emits four
separate `check-sat` obligations:

| Obligation | SAT means |
| --- | --- |
| `requires_defined` | The precondition may overflow. |
| `requires_satisfiable` | Some input satisfies the precondition. |
| `ensures_false` | A defined postcondition is false. |
| `ensures_overflow` | The postcondition or substituted body overflows. |

`audit_encoding` parses and interprets the **emitted SMT assertions** without
using the contract evaluator. It compares all four obligations for every
finite tuple. `audit_encoding_with_source` also accepts a separate source-AST
evaluator callback. Missing, malformed, or disagreeing formulas fail closed;
finite parity is encoding evidence, not a proof for all `i64` inputs.

`parse_solver_result` accepts exactly four status lines, zero exit, and empty
stderr. `Unsat` is only a protocol observation. The CLI additionally checks
source and encoding parity, fixture responses, and a caller supplied solver
digest pin before it can report a conditional proof for the selected function.
`validate_solver_witness` parses a complete `get-value` response and checks
names, sorts, ranges, and the claimed violation. `observe_solver` limits each
output stream to 16 KiB and includes stdin, exit, and pipe closure in its
deadline. On Linux it also limits address space to 512 MiB.

Conditional values use SMT `ite`; their definedness is
`(and guard_defined (ite guard_term then_defined else_defined))`.
The independent SMT interpreter selects the `ite` branch. The four-obligation
order, i64 range constraints, finite scope, witness validation and solver trust
contract are unchanged. Conditional source admission, its independent AST
oracle and the 32-level/1,024-node source caps belong to the CLI crate.
