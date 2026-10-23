# S2a language representation contract

This indentation grammar is an implementation candidate, pending the R14 syntax
study. Spans are zero-based half-open UTF-8 byte offsets. Four spaces are the
canonical indentation. This contract describes S2a; it does not close R02/R11.

```cg
pub variant Reply:
    Empty
    Value(i64, Bool)

fn unwrap(x: Option<i64>) -> i64:
    match x:
        Option<i64>.Some(value):
            value
        Option<i64>.None:
            0
```

Variant cases have ordered positional payloads (including zero payloads).
Constructors are qualified by their type: `Reply.Value(3, true)` and
`Reply.Empty()`; generic builtins require explicit concrete arguments:
`Option<i64>.Some(3)`, `Option<i64>.None()`,
`Result<i64, String>.Err("failure")`. There is no contextual generic inference.
`Option<T>` has Some(T)/None; `Result<T,E>` has Ok(T)/Err(E). Their identities
are reserved `cgx2.core.Option` and `cgx2.core.Result`.

`match` is a statement with branch suites, usable as a function's tail.
Every arm is a qualified case, optionally followed by positional binding names,
or `_`. Only a selected arm executes; the scrutinee is evaluated once. Bindings
are immutable, local to one arm, may shadow enclosing bindings, and may not
repeat in that arm. `_` in a payload discards it. Nested patterns and guards
are unsupported and diagnosed. Every case must be covered, or a final `_`
must cover the remainder. Duplicate cases and arms after exhaustive coverage
are errors. Branch result types must agree. Ordinary checked arithmetic,
runtime contracts, and effect/capability checks retain their existing behavior.

Public types export all their cases/fields; private types cannot occur in public
signatures or public payloads. Duplicate types, cases, fields, bindings, wrong
payload count/types, unknown and inaccessible types/cases are errors. User
variants are concrete in S2a. Records may contain variants and variants may
contain records/variants. Cycles through record/variant payloads or concrete
builtin applications are rejected rather than expanding recursive types.
Nominal identity is the defining module plus declaration name, independent of
the import spelling. Two modules' same-named declarations are distinct.

## Next-stage syntax and representation

`TypeSyntax { path, arguments, span }` remains the type-application shape.
Named generic parameters use `<T: Eq + Show, U>` on record, variant, and
function declarations. Parameters retain a name, constraints with path spans,
and a declaration span. S2a parses/formats/indexes these declarations but
rejects their semantics explicitly; substitution, inference and constraint
solving belong to S2b. Builtin Option/Result are the exception with fixed arity.

Collection candidates are `List<T>` / `Array<T>` types, `[a, b]` literals and
`values[index]` checked-index syntax. AST nodes preserve every child expression.
Collection execution and bounds semantics remain S2b; array proof obligations
remain S3-V. Annotated range loops place `invariant expression` and
`decreases expression` at the beginning of their suite. These clauses retain
source expressions/spans and format canonically. S2a rejects annotated-loop
execution; proof generation is S3-V. No syntax admission implies proof.

## Compatibility and evidence

The completed delivery's evidence report records exact public interface
changes, consumer/target admission matrix, commands and results. Notebook and
verification consumers must reject new execution/proof forms with source
locations until S3. Advisory consumers must traverse children or reject the
profile explicitly. Python may issue a target-specific error for variants;
Rust native and the checked interpreter must execute the admitted profile.
Root Cargo files, runtime/foreign crates, shared plans and ledgers are unchanged.

The implemented public interface migration, finite-type bounds, admission matrix,
source/target evidence, and exact next-stage boundaries are recorded in
[s2a-evidence/README.md](s2a-evidence/README.md). The semantically checked type
representation covers concrete admitted types; generic binders/applications and
named constraints remain syntax/API representations until S2b. Syntax-only API
metadata explicitly carries `semantics_status = "syntax-only"` and unchecked
loop clauses; it is not a typed or proof summary.
