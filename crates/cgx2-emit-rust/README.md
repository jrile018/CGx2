# cgx2-emit-rust

`cgx2-emit-rust` emits a standalone Rust 2024 program from a parsed, successfully checked CGx2 scalar module:

```rust
use cgx2_emit_rust::emit_rust;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

let module = parse_module("let x = 2\nx + 1\n").unwrap();
let checked = check_module(&module).unwrap();
let rust = emit_rust(&module, &checked).unwrap();
std::fs::write("generated.rs", rust).unwrap();
```

Compile the result with `rustc --edition=2024 generated.rs -o generated`; run `./generated --example-argument`. The generated file uses only the Rust standard library. Program arguments follow the executable name and are visible through `arg_count()` and zero-based `arg(index)`.

The profile accepts scalar `i64`, `Bool`, `String`, and `Unit`; script statements; `let`, mutable assignment and `+=`; direct function calls; explicit and tail returns; and simple module headers. A function module requires a zero-argument `main`. A script prints its final expression if it is not `Unit`. Booleans print `true` or `false`; strings retain CGx2's supported escapes and Unicode. Source names are encoded deterministically to avoid Rust keywords and generated helper names. The output is line-oriented and intended to be readable and editable.

Arithmetic uses checked signed `i64` operations in debug and optimized builds. Overflow, division by zero, invalid argument indices, excess execution steps, and excess call depth produce a nonzero exit and a defined message on stderr. Recursive call graphs are rejected because the current generated program does not preserve the CLI's recursion behavior precisely.

`emit_rust` rechecks the AST and compares every part of its fresh `TypedModule` with the supplied summary, including function signatures and expression span types. It returns source byte spans with `CGRS001` for unsupported target forms, `CGRS002` for a stale or invalid typed summary, and `CGRS003` for unrepresentable literals. The typed summary has no source fingerprint; an AST with the same valid typed summary can pass the comparison, but it is always rechecked and cannot gain unchecked types.

Memory regions, owned values, `no_gc`, loops, named arguments, qualified or unknown calls, foreign calls, effects, and Python/PyTorch execution are outside this target. This is a scalar source emitter, not the full native backend. Edits to generated Rust do not round-trip to `.cg`.

Run `cargo test -p cgx2-emit-rust --offline` for differential tests against `cgx2-cli::run_source` in debug and optimized Rust builds. Those tests compare output, success or failure, and error messages for deterministic fixtures.
