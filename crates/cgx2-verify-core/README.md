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


## Additive scalar v2 API

`scalar_v2` preserves the frozen root API while adding `Expr::Mul`, `Expr::Div`
and `Expr::Let(name, initializer, body)`. Bindings are eager and lexical, including
shadowing. Division truncates toward zero; zero and MIN/-1 fail with the existing
`EvalError::Overflow` definedness outcome. SMT uses QF_UFNIA, guarded mathematical
integers and sign-adjusted division; the finite audit independently interprets
shared definitions, SMT `let`, multiplication and nonnegative division. Combining lexical bindings
and unmodeled calls is explicitly rejected by `SmtError::UnmodeledLocalBinding`.

`LiteralAccumulator { name, initial, index, start, end, increment, result }`
provides library-only `.expand(max_expansion)`. A bound 1..=64 is mandatory;
wide ranges are rejected before tree allocation. Reversed/empty half-open ranges
execute no updates, but still eagerly evaluate `initial`. The index is scoped
only to each update and cannot capture an outer index in `result`. Callers must
account for cumulative expansion across multiple builders. Source admission is
blocked by the shared Mutate effect checker, as described in the verifier CLI
README. Array/invariant proofs and new type support remain outside this profile.

Protocol handling, finite search and witness validation use one shared source
compiled for the original and v2 types; the version-specific expression model,
evaluator, encoding and independent SMT interpreter are separate modules. This
keeps the old exhaustive enums stable while preserving security checks in both.

## Bounded scalar v2 SMT emission

`scalar_v2::emit_smtlib` and `emit_witness_smtlib` use `SmtLimits::default()`:
4,096 combined expression nodes and parameters, expression depth 128 (root depth
one), and 1,048,576 output bytes per complete script. These are hard ceilings;
`emit_smtlib_with_limits` / `emit_witness_smtlib_with_limits` accept smaller
positive `nodes`, `depth`, and `output_bytes` budgets. Larger/zero budgets return
`SmtError::InvalidLimits`. `check_smt_limits` checks shape iteratively before
recursive validation/sorting; total identifier UTF-8 bytes across parameters and
expression occurrences must also fit `output_bytes / 2`, before hex encoding.
The emitter checks each output fragment before copying/reserving it, including
identifier encoding, declarations, definitions, obligations and witness names.
It returns `SmtError::LimitExceeded { resource, limit }` without returning a
partial script. Allocation is bounded during construction, rather than checked
only after constructing expanded text. Caller-supplied AST allocation/destruction
and process-wide allocator exhaustion are outside this emission API.

Each value/definedness pair uses fresh, sequential zero-argument `define-fun`
bindings. Child references have bounded-length internal names (`cgx2_t_N`,
`cgx2_d_N`); parameters and UF names use separate hex-encoded namespaces.
Lexical environments resolve each source local to its initializer reference,
with the initializer evaluated before shadowing. Definitions occur once, in
dependency order; output grows linearly in nodes with a logarithmic identifier
length factor. Requires/ensures and witness queries reuse those definitions.
Branch definedness still selects only the active branch. Arithmetic remains
eager and checked: invalid children cause a dummy zero parent value while
propagating false definedness. This bounds inactive integer intermediates in the
independent interpreter without admitting any failed source operation. Signed
division truncates toward zero; division by zero and MIN / -1 remain failures.
Global UF range assumptions and conservative local-plus-UF rejection remain.
The frozen root API is unchanged.

The scalar v2 audit parses the actual emitted definitions and four assertions,
evaluates each definition once per tuple, checks its declared sort, and rejects
unknown/forward/duplicate definitions and definitions after obligations. It does
not regenerate the script or skip SMT evaluation. Script bytes share the 1 MiB
ceiling; existing per-term limits remain 256 KiB, 32,768 tokens, depth 256.
Over-budget input contracts produce `AuditError::InvalidInput`; malformed or
over-budget scripts produce `MalformedFormula`. Finite/source/SMT parity is
encoding evidence, never trusted live solver acceptance.
