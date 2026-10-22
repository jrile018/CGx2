# CGx2 CLI execution preview

This crate provides a small interpreter for the frozen Wave 1 `.cg` syntax. It is a preview of `cgx2` execution, not the planned compiler, static checker, native backend, memory runtime, or proof system.

## Commands

From the workspace root:

```sh
cargo run -p cgx2-cli --offline -- run program.cg -- first "second argument"
cargo run -p cgx2-cli --offline -- program.cg -- first
cargo run -p cgx2-cli --offline -- fmt program.cg
```

`--` separates program arguments from CLI arguments; `arg(0)` reads the first argument after it. `fmt` writes canonical source to stdout and does not change the file. A successful run writes the final non-`Unit` value and one newline to stdout. Errors go to stderr and exit nonzero. File errors include the path; parser diagnostics retain `CGS` codes and runtime diagnostics use `CGR` codes. Runtime and syntax spans are zero-based, half-open UTF-8 byte offsets.

## Executable subset

- A script runs its top-level statements in order. A function module calls exactly one zero-argument `main`. Missing or duplicate `main` definitions are errors.
- Values are `i64`, `Bool`, `String`, and `Unit`. Supported expressions are literals, local names, calls, and `+`, `-`, `*`, `/` on `i64`. Integer literals may contain `_`. Arithmetic is checked; division by zero and overflow are errors. Strings support `\n`, `\r`, `\t`, `\\`, and `\"` escapes.
- `let`, `let mut`, `=`, `+=`, expression statements, and function `return` work. The last expression in a script or function body supplies its value; other final statements yield `Unit`. Function calls have local parameter and binding scopes. Annotations may name only the four scalar types and are checked at runtime; unannotated mutable bindings also retain their initial value type. `arg_count()` returns the number of program arguments; `arg(i64)` returns an argument or errors on a negative or missing index. Those names are reserved.
- Execution stops after 1,000,000 statements or expressions, or at a call depth of 64. The depth guard also protects the host stack from recursive programs.

`for` loops, qualified names, generic and non-scalar types, effects, capabilities, contracts, ownership, regions, imports, and foreign calls are unsupported. The interpreter reports encountered unsupported constructs with `CGR009`; it does not provide static type safety or native performance. `check`, `build`, `test`, `verify`, `emit`, notebook commands, package resolution, and structured JSON diagnostics are later work.
