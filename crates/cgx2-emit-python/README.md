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

The emitted file runs with Python 3 and prints a script's last non-`Unit` expression or a zero-argument `main` result. Arguments after the Python file path supply `arg_count()` and `arg(index)`. The profile covers scalar `i64`, `Bool`, `String`, and `Unit` values; `let`, mutable assignment, `+=`, arithmetic, scalar comparisons, checked `range(start, end)` loops, direct calls, explicit returns, tail expressions, and simple module headers. It rejects function modules without a zero-argument `main` and all imports.

The generated prelude checks signed `i64` bounds after `+`, `-`, `*`, and `/`. Division truncates toward zero; division by zero and overflow raise Python errors. The prelude also formats CGx2 Booleans as `true`/`false`, handles program arguments, and prints exactly one newline after a non-`Unit` result. Source identifiers stay readable when they are safe Python identifiers; keywords, reserved helper names, and non-ASCII identifiers receive deterministic byte-based names. Strings are decoded using the preview CGx2 escapes and re-quoted for Python.

Checked scalar `requires` expressions run in source order before the function body. `ensures` expressions run in source order after the body returns, with `result` bound to its value, including explicit returns. A false condition raises `_cgx2_ContractError` with `code` (`CGR015` or `CGR016`) and a zero-based, half-open UTF-8 byte `span`; the error message contains the same code and span. This matches the CLI's failure class and ordering. Arithmetic overflow maps CLI `CGR005` to Python `OverflowError`; division by zero maps `CGR006` to `ZeroDivisionError`. Neither failure path prints a successful result. Traceback text is Python-specific.

The emitter accepts explicit empty effects/capabilities and declarations limited to `Mutate` (local assignment), `ReadArgs`, and `ArgsRead` when the checker verifies them. It also checks inferred transitive effects. These annotations are a compile-time target gate, not a runtime grant. `Foreign`, `ManagedAlloc`, `Collect`, `Unknown`, `MemoryCollect`, and `ForeignCall` are rejected, including when declared but unused.

`emit_python` returns `Vec<EmitDiagnostic>` with source byte spans: `CGE001` for unsupported target forms such as non-range `for`, `List<T>`, qualified names/calls, imports, or a missing entry point; `CGE002` when the supplied `TypedModule` differs from a fresh check of the AST; `CGE003` for unrepresentable literals; and `CGE004` for unsupported effect/capability metadata. Declaration errors point to the unsupported name, while inferred unsupported effects point to the function. The frozen `TypedModule` contains types and spans but no source fingerprint. Two different ASTs with identical typed summaries cannot be distinguished; the emitter always rechecks the supplied AST, so such a match cannot make it emit code under incorrect types.

Regions, `no_gc`, owned borrows, foreign calls, and typed PyTorch are outside this source profile. PyTorch access remains a separate bridge target. This crate is not a Python interoperability layer or a claim of general CGx2 portability. Generated edits do not round-trip to CGx2.

Verification: `cargo test -p cgx2-emit-python --offline` runs Python compilation and execution tests plus paired fixtures against `cgx2-cli`'s evaluator.

## Checked local packages

`emit_package_python(&CheckedPackage)` returns a deterministic map of flat `.py` files without writing or executing them. Obtain the snapshot with `cgx2_package::load_checked_package(root)`. `write_package_python(&CheckedPackage, output_dir)` writes the same files into a **new** directory; it refuses traversal, symlink parents, and an existing output path. Both APIs verify the manifest and source bytes against the checked snapshot before and after generation. Run the result with `python3 output_dir/__main__.py [args...]`.

The package profile supports an acyclic graph of manifest-listed modules with explicit imports, public scalar calls, local private functions, and a zero-argument `main` in the manifest entry module. Generated module file names and imported call aliases are deterministic encodings of CGx2 names; `__all__` lists public functions, and private functions receive internal Python names. Python itself does not enforce module privacy against direct introspection. The output tree includes one readable `_cgx2_support.py` with checked scalar helpers and a shared contract error class, so no installed CGx2 Python runtime package is required. The writer never imports or runs generated code.

Only `i64`, `Bool`, `String`, and `Unit` signatures and expressions are supported. Checked `for item in range(start, end):` loops evaluate each bound once, support nesting and lexical shadowing, and perform checked `i64` arithmetic. The shared `_cgx2_support.py` counts each source statement, expression, range iteration, and local call frame; exhaustion raises `CGR010` at 1,000,000 steps or `CGR011` at 64 frames. Generated statement comments carry deterministic CGx2 byte spans for source mapping. The translator rejects script statements in package modules, foreign bindings, tensor and memory operations, `no_gc`, nonscalar signatures (including imported public functions), and other loop iterators with file and source span diagnostics. It does not preserve CGx2 memory regions, Python/PyTorch foreign allocations, or Python traceback text. Generated edits do not round-trip to `.cg`.

The range-loop body checker currently accepts pure calls to functions in the same module. A qualified cross-module call inside the loop body is rejected by the checker as `CGT012` before emission; cross-module pure calls elsewhere in the package are supported. The subprocess parity tests use Python 3 with a 15-second timeout per generated execution.
