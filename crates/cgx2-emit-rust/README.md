# cgx2-emit-rust

`cgx2-emit-rust` emits Rust 2024 from a parsed, successfully checked CGx2 scalar module or bounded memory script:

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

`emit_package_rust(&CheckedPackage)` emits a single Rust 2024 program from `cgx2-package::load_checked_package`. It lowers dependency-first modules and encodes each fully qualified function name as a collision-free Rust identifier, including dotted names, Unicode, and Rust keywords. Calls to public functions require an explicit listed import; private same-module calls remain available. `PackageEmitDiagnostic` adds a manifest-relative source path to the usual code, message, and byte span. The package entry needs a zero-argument `main` returning `i64`, `Bool`, `String`, or `Unit`. Package modules cannot contain script statements. The package caller checks source hashes before saving output.

For checked memory scripts and zero-argument, `Unit`-returning local memory functions, `emit_rust` selects a separate native path. It supports literal `usize` byte sizes; `memory.region(mode: auto|manual, limit: N)`; `alloc_bytes`, `owned_bytes`, `move`, `promote_owned`, `copy_to_owned`, `borrow`/`borrow_mut`, `use`, `drop`, `end_borrow`, `collect`, and `close`; nested `no_gc(budget: N)`; and direct local calls through `main()`. Each call gets its own bindings and unroots its remaining managed values on return, including error paths. All calls share one `cgx2_runtime::Session` and the one-million-step limit. The auto collection trigger is 1 MiB. Runtime failures carry the CGx2 operation byte span; successful programs return exit code 0 and print nothing. `cgx2c build` embeds the runtime source at compiler build time and needs only Rust 1.98 `rustc` at execution; no source checkout is needed. For manual compilation of `emit-rust` output, provide a version-matched `cgx2-runtime` rlib, for example from a checkout:

```sh
rustc --edition=2021 --crate-name cgx2_runtime --crate-type=lib crates/cgx2-runtime/src/lib.rs -o /tmp/libcgx2_runtime.rlib
rustc --edition=2024 --crate-name cgx2_generated generated.rs --extern cgx2_runtime=/tmp/libcgx2_runtime.rlib -o /tmp/generated
```

Scalar `requires` and `ensures` expressions using Boolean literals, `i64` literals, parameters, `result`, arithmetic, and scalar comparisons execute before the body and after a successful return. A failure exits nonzero with `CGR015` or `CGR016` and the condition's byte span. A source-valid pure function call inside a contract receives `CGRS001` with its source span; it is never silently omitted. Explicit effects and capabilities remain subject to the existing checker before emission. Package emission applies the same contract handling to every function.

Cross-call memory handles, parameters or non-`Unit` results on memory functions, recursion, dynamic sizes or calls, loops, foreign calls, mixed scalar/memory bodies, and Python/PyTorch execution remain unsupported. Invalid checked source and unsupported target forms receive source-located diagnostics. Set `CGX2_MEMORY_REPORT=/path/to/report.json` when running a generated memory binary to write a native report after execution, including allocation failures. Without it, stdout, stderr, exit status, and program arguments retain their usual behavior. The report wraps the runtime's schema v1 `MemoryReport` in native report schema v1 with `status` (`ok` or `error`) and `diagnostic_code` (`null` or the stable `CGR` code). It is capped at 1 MiB and installed through a temporary file; invalid paths fail before execution with `CGR017` and do not replace the binary or an existing report. No report is created when checking or compilation fails. Per-region and total byte, failure, and collection counters come from the same runtime session that ran the program. Pause duration and histogram fields measure wall-clock time and may differ between debug, release, and interpreter runs. This supports the frozen memory-script and zero-argument `Unit` local-function profile; it does not establish parity for source object graphs, foreign memory, or other unsupported forms. Generated source is a bounded profile, not the full native backend or an SMT proof. Edits to generated Rust do not round-trip to `.cg`.

Run `cargo test -p cgx2-emit-rust --offline` for differential tests against `cgx2-cli::run_source` in debug and optimized Rust builds. Those tests compare output, success or failure, and error messages for deterministic fixtures.
