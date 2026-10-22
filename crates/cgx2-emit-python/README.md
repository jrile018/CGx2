# cgx2-emit-python

This library emits readable Python from a parsed and successfully checked CGx2 module:

```rust
use cgx2_emit_python::emit_python;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

let module = parse_module("let x = 2\nx + 1\n").unwrap();
let typed = check_module(&module).unwrap();
let python = emit_python(&module, &typed).unwrap();
```

The emitted file runs with Python 3.14 and prints a script's last non-`Unit` expression or a zero-argument `main` result. Arguments after the Python file path supply `arg_count()` and `arg(index)`. The profile covers scalar `i64`, `Bool`, `String`, and `Unit` values; `let`, mutable assignment, `+=`, arithmetic, direct calls, explicit returns, tail expressions, and simple module headers. It rejects function modules without a zero-argument `main`.

The generated prelude checks signed `i64` bounds after `+`, `-`, `*`, and `/`. Division truncates toward zero; division by zero and overflow raise Python errors. The prelude also formats CGx2 Booleans as `true`/`false`, handles program arguments, and prints exactly one newline after a non-`Unit` result. Source identifiers stay readable when they are safe Python identifiers; keywords, reserved helper names, and non-ASCII identifiers receive deterministic byte-based names. Strings are decoded using the preview CGx2 escapes and re-quoted for Python.

`emit_python` returns `Vec<EmitDiagnostic>` with source byte spans: `CGE001` for unsupported target forms such as `for`, `List<T>`, qualified names/calls, or a missing entry point; `CGE002` when the supplied `TypedModule` differs from a fresh check of the AST; and `CGE003` for unrepresentable literals. The frozen `TypedModule` contains types and spans but no source fingerprint. Two different ASTs with identical typed summaries cannot be distinguished; the emitter always rechecks the supplied AST, so such a match cannot make it emit code under incorrect types.

Regions, `no_gc`, owned borrows, effects, foreign calls, and typed PyTorch are outside this source profile. PyTorch access remains a separate bridge target. This crate is not a Python interoperability layer or a claim of general CGx2 portability. It does not implement the CLI's execution step and call-depth limits; recursive programs may fail at different points. Generated edits do not round-trip to CGx2.

Verification: `cargo test -p cgx2-emit-python --offline` runs Python compilation and execution tests plus paired fixtures against `cgx2-cli`'s evaluator.
