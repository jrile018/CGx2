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

For checked memory scripts and zero-argument, `Unit`-returning local memory functions, `emit_rust` selects a separate native path. It supports literal `usize` byte sizes; `memory.region(mode: auto|manual, limit: N)`; `alloc_bytes`, `alloc_object(region, payload_bytes, reference_slots)`, `reference(owner, literal_slot)`, `set_reference(owner, literal_slot, target)`, `clear_reference(owner, literal_slot)`, `use_reference(ref)`; `owned_bytes`, `move`, `promote_owned`, `copy_to_owned`, `borrow`/`borrow_mut`, `use`, `drop`, `end_borrow`, `collect`, and `close`; nested `no_gc(budget: N)`; and direct local calls through `main()`. The frozen checker bounds object payloads to 1 MiB and reference slots to 256. These calls use `cgx2_runtime::Session` for rooting, checked edges, collection, and release. Each local call gets its own bindings and unroots remaining managed values on return, including error paths. All calls share one session and a one-million-step limit. The auto collection trigger is 1 MiB. Runtime failures carry the CGx2 operation byte span; successful programs return exit code 0 and print nothing. `cgx2c build` embeds runtime source at compiler build time and needs only Rust 1.98 `rustc` at execution; no source checkout is needed. For manual compilation of `emit-rust` output, provide a version-matched `cgx2-runtime` rlib, for example from the checkout:

```sh
rustc --edition=2021 --crate-name cgx2_runtime --crate-type=lib crates/cgx2-runtime/src/lib.rs -o /tmp/libcgx2_runtime.rlib
rustc --edition=2024 --crate-name cgx2_generated generated.rs --extern cgx2_runtime=/tmp/libcgx2_runtime.rlib -o /tmp/generated
```

Scalar `requires` and `ensures` expressions using Boolean literals, `i64` literals, parameters, `result`, arithmetic, and scalar comparisons execute before the body and after a successful return. A failure exits nonzero with `CGR015` or `CGR016` and the condition's byte span. A source-valid pure function call inside a contract receives `CGRS001` with its source span; it is never silently omitted. Explicit effects and capabilities remain subject to the existing checker before emission. Package emission applies the same contract handling to every function.

Cross-call memory handles, parameters or non-`Unit` results on memory functions, recursion, dynamic sizes or calls, loops, foreign calls, mixed scalar/memory bodies, and Python/PyTorch execution remain unsupported. Arbitrary typed objects, concurrency, and foreign-handle safety are outside this profile. Invalid checked source and unsupported target forms receive source-located diagnostics. Set `CGX2_MEMORY_REPORT=/path/to/report.json` when running a generated memory binary to write a native report after execution, including allocation failures. Without it, stdout, stderr, exit status, and program arguments retain their usual behavior. The report wraps the runtime's schema v1 `MemoryReport` in native report schema v1 with `status` (`ok` or `error`) and `diagnostic_code` (`null` or stable `CGR` code). It is capped at 1 MiB and installed through a temporary file; invalid paths fail before execution with `CGR017` and do not replace the binary or an existing report. No report is created when checking or compilation fails. Compare interpreter `MemoryReport::to_json()` with the native report's `memory` field for region and total committed, live, peak, failure, collection, and release (`total_collected_bytes`) counters. Ignore pause timing and histogram samples, which measure wall-clock time. The interpreter `run_checked_source_with_failure` API now exposes a partial runtime report, so failure comparisons include code, source span, semantic memory counters, and observed rollback state. The parity test uses the explicit `cgx2c --test-fail-reserve N` build option; ordinary generated binaries have no active injection hook. Generated source is a bounded profile, not a full native backend or an SMT proof. Edits to generated Rust do not round-trip into `.cg`.

Run `cargo test -p cgx2-emit-rust --offline` for differential tests against `cgx2-cli::run_source` in debug and optimized Rust builds. Those tests compare output, success or failure, and error messages for deterministic fixtures.


The native failure report also carries `failure_report_schema_version: 1`, `file`, `span`, `rollback_preserved`, and additive `operation_failure` and `final_unwind` snapshots. Each snapshot has observed live binding `rooted_handles`, occupied `rooted_edges`, and a runtime memory report with per-region and total counters including `collection_count`. The first snapshot precedes local binding cleanup; the second follows complete function and top-level binding cleanup. Earlier successful operations remain visible. Counts become null when a root's reference slots exceed the 1,024-slot observation bound. `cgx2c build` embeds the source file in generated code and supplies the opt-in `--test-fail-reserve N` build flag for host allocation faults. Raw `emit_rust` output uses `<source>` unless compiled with `CGX2_SOURCE_FILE`. No failure report is written unless `CGX2_MEMORY_REPORT` is set at execution. Static rejections produce no runtime report.

**Root observation limit:** generated `final_unwind` currently writes zero source-binding counts after cleanup; it does not inspect the runtime root table. This is binding-lifecycle parity, not independent evidence that all runtime roots were released.
