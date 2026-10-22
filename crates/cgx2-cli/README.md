# CGx2 CLI execution preview

This crate provides a checked command-line preview for the frozen Wave 1 `.cg` syntax. The `check` command uses the Wave 2 semantic checker; `run` performs the same check before entering the small interpreter. It is not the native compiler, memory runtime, or proof system. The public `cgx2_cli::run_source(source, args)` function remains a low-level interpreter API and does not perform the static check.

## Commands

From the workspace root:

```sh
cargo run -p cgx2-cli --offline -- check program.cg
cargo run -p cgx2-cli --offline -- run program.cg -- first "second argument"
cargo run -p cgx2-cli --offline -- program.cg -- first
cargo run -p cgx2-cli --offline -- fmt program.cg
cargo run -p cgx2-cli --offline -- check program.cg --diagnostic-format=json
```

`check` parses and checks names, scalar/list types, function calls and returns, mutability, and a bounded memory-source script profile through ownership operations. It does not validate effects, capabilities, contracts, package dependencies, or runtime memory budgets. A semantically valid `for` loop can pass `check` while the interpreter rejects it with `CGR009`. Memory scripts can pass `check`, but `run` rejects memory execution with `CGR009`. `--help` shows this scope at the command line.

`--` separates program arguments from CLI arguments; `arg(0)` reads the first argument after it. `--diagnostic-format=json` may appear before the `--` delimiter on `check` or `run`; after the delimiter it is a program argument. `fmt` writes canonical source to stdout and does not change the file. A successful run writes the final non-`Unit` value and one newline to stdout. Errors go to stderr and exit nonzero without a success value. Parser diagnostics retain `CGS` codes, semantic diagnostics use `CGT` codes, and runtime diagnostics use `CGR` codes. Human and JSON Lines diagnostics share the same message and source location. Each JSON line has `code`, `severity`, `message`, `file`, and `span` fields; source spans are zero-based, half-open UTF-8 byte offsets. File and command errors without a source span use `null`.

## Executable subset

- A script runs its top-level statements in order. A function module calls exactly one zero-argument `main`. Missing or duplicate `main` definitions are errors.
- Values are `i64`, `Bool`, `String`, and `Unit`. Supported expressions are literals, local names, calls, and `+`, `-`, `*`, `/` on `i64`. Integer literals may contain `_`. Arithmetic is checked; division by zero and overflow are errors. Strings support `\n`, `\r`, `\t`, `\\`, and `\"` escapes.
- `let`, `let mut`, `=`, `+=`, expression statements, and function `return` work. The last expression in a script or function body supplies its value; other final statements yield `Unit`. Function calls have local parameter and binding scopes. Annotations may name only the four scalar types and are checked at runtime; unannotated mutable bindings also retain their initial value type. `arg_count()` returns the number of program arguments; `arg(i64)` returns an argument or errors on a negative or missing index. Those names are reserved.
- Execution stops after 1,000,000 statements or expressions, or at a call depth of 64. The depth guard also protects the host stack from recursive programs.

`for` loops, qualified names, generic and non-scalar types, effects, capabilities, contracts, ownership, regions, imports, and foreign calls are unsupported by the interpreter. It refuses any parsed memory construct before evaluation, including in an uncalled function or via the low-level `run_source` API, with `CGR009`. It does not provide native performance. `build`, `test`, `verify`, `emit`, notebook commands, and package resolution are later work.
