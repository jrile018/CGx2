# cgx2-sema

`cgx2-sema` checks the unchanged Wave 1 `cgx2_syntax::Module` AST. Parse first with `cgx2_syntax::parse_module`, then call `check_module(&module)`. Success returns a `TypedModule`; failure returns all collected `TypeDiagnostic` values. Each diagnostic has a stable `code`, a `message`, and a zero-based, half-open UTF-8 byte `span` into the original source.

`TypedModule.functions` is a `BTreeMap<String, FunctionSignature>` keyed by function name. It includes the reserved preview builtins `arg_count() -> i64` and `arg(i64) -> String`, which have synthetic `Span { start: 0, end: 0 }`. `FunctionSignature` exposes `public`, ordered `parameters`, `return_type`, and declaration `span`. Private functions without a return annotation infer their return type from the final statement. Unannotated recursive return inference is unsupported; annotate those functions. `TypedModule.expressions` is a `SpanTypes` index: `get(span)` retrieves an expression's `Type`, and `iter()` yields `(Span, &Type)` pairs in byte-offset order. Failed expressions have no entry.

The checked subset has `i64`, `Bool`, `String`, `Unit`, and nested `List<T>` types; local `let` inference and annotations; lexical scopes in `for` bodies; immutable and mutable assignment; `i64` arithmetic; calls; explicit returns; and tail-expression returns. A `for` iterator must be `List<T>`, and its binding has type `T`. Public functions require an explicit return annotation. Integer literals must fit signed `i64`; arithmetic overflow remains a runtime error. The checker recognizes only the two argument builtins above.

| Code | Meaning |
| --- | --- |
| `CGT001` | Unknown or duplicate name |
| `CGT002` | Unknown or unsupported type |
| `CGT003` | Incompatible types, including arithmetic or call arguments |
| `CGT004` | Assignment to an immutable binding |
| `CGT005` | Wrong call arity |
| `CGT006` | Missing public return annotation |
| `CGT007` | Integer literal outside signed `i64` |

This crate does not execute code, check runtime overflow, implement effects or capabilities, enforce ownership or region safety, or provide a `cgx2 check` CLI command. Function values and qualified names are outside this preview. The syntax parser has no list literal, so list iteration is checked from typed parameters or other future expressions.
