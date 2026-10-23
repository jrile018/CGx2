# cgx2c native compiler preview

From the repository root, with Rust 1.98 and `rustc` installed:

```sh
cargo run -p cgx2c --offline -- build crates/cgx2c/examples/hello.cg --out /tmp/cgx2-hello
/tmp/cgx2-hello
cargo run -p cgx2c --offline -- emit-rust crates/cgx2c/examples/hello.cg --out /tmp/cgx2-hello.rs
rustc --edition=2024 /tmp/cgx2-hello.rs -o /tmp/cgx2-hello-manual
```

If the emitted `.rs` filename contains spaces or punctuation, add `--crate-name cgx2_generated` to the manual `rustc` command. `cgx2c build` uses a fixed temporary Rust filename and accepts such output paths directly.

`build FILE.cg --out BINARY [--release] [--force] [--rustc PATH]` checks and emits the scalar or bounded memory-script profile, then invokes `rustc` with argument vectors. Debug builds pass `-C opt-level=0`; `--release` passes `-C opt-level=3` to both the generated program and, for memory scripts, its runtime. The default compiler is `rustc` on `PATH`; `CGX2C_RUSTC` or `--rustc PATH` selects another executable. `emit-rust FILE.cg --out FILE.rs [--force]` writes the deterministic, readable emitter output. Both commands require an explicit output. Existing output requires `--force`; an output that is the input file and existing symlink outputs are rejected. Source is limited to 4 MiB and emitted Rust to 32 MiB.

For memory scripts, `cgx2c` embeds the exact `cgx2-runtime` source files present when it was compiled. `build` writes those files to its private temporary directory, compiles one rlib with the selected profile, and links generated Rust to it. A copied or installed `cgx2c` binary needs a compatible Rust 1.98 `rustc` executable, but no source checkout or network access. Rebuild `cgx2c` to pick up later runtime source changes; the build receipt records `runtime_sha256` over the embedded file names and contents (`none` for scalar builds). There is no runtime artifact cache. `emit-rust` writes source that requires `--extern cgx2_runtime=...`; see the emitter README for manual `rustc` commands. The supported memory operations and contract expressions are listed there. Native memory binaries expose an opt-in JSON report; failures still exit nonzero with an operation byte span.

To build the checked three-module example, run from the repository root:

```sh
cargo run -p cgx2c --offline -- build --package crates/cgx2c/examples/package-three --out /tmp/cgx2-package-demo
/tmp/cgx2-package-demo  # prints 42
cargo run -p cgx2c --offline -- emit-rust --package crates/cgx2c/examples/package-three --out /tmp/cgx2-package-demo.rs
rustc --edition=2024 --crate-name cgx2_generated /tmp/cgx2-package-demo.rs -o /tmp/cgx2-package-manual
```

`build --package ROOT --out BINARY [--release] [--force] [--rustc PATH]` and `emit-rust --package ROOT --out FILE.rs [--force]` require a manifest-listed, acyclic local package. The entry module must define a zero-argument `main` returning `i64`, `Bool`, `String`, or `Unit`; imported calls require explicit imports and public scalar signatures. The compiler checks the exact source bytes used by the emitter, verifies hashes and canonical paths again before installing output, and rejects output identity with the manifest or any listed source. Existing artifacts survive validation and compiler failures. The build receipt includes the manifest SHA-256, each source path and SHA-256, `rustc` identity/target, profile, and output path. The generated Rust is deterministic for a fixed checked package.

Generated files and binaries are staged in a private directory alongside the requested output and removed on error. On Unix, installation without `--force` uses an atomic hard link that cannot replace an existing output; with `--force`, rename atomically replaces a regular file. The preceding symlink check is best effort against concurrent path changes. Filesystems without hard link support cannot use the no-force installation. Windows replacement behavior depends on its rename semantics; this preview is verified on Unix.

After a successful build, stdout contains a build metadata receipt with SHA-256 of the source bytes, selected `rustc` path, `rustc -vV` version and host target, profile, embedded runtime SHA-256 for memory builds, and output path. Capture that line if persistent provenance is needed. Version and target say `unavailable` if the selected compiler cannot answer `-vV`; a wrapper may report its own identity. This preview does not record linker flags, external environment, or a reproducible build attestation.

Source diagnostics retain the syntax (`CGS`), type (`CGT`/`CGM`/`CGO`), or Rust target (`CGRS`) code and UTF-8 byte span. `CGXC001` is command usage, `CGXC002` source I/O, `CGXC004` an unsafe or occupied output, `CGXC005` a `rustc` failure, and `CGXC006` staging/install I/O. Compiler stderr is capped at 8 KiB and control characters are sanitized.

The compiler accepts checked scalar and bounded memory Rust-emitter profiles. Memory modules require zero-argument `main() -> Unit` and permit direct acyclic calls to local `Unit` functions with explicit `i64` scalar parameters. Parameters may be forwarded or supplied only as the byte size in `memory.alloc_bytes(region, size)` and `memory.owned_bytes(size)`. The checker validates call arity/types and transitive effects/capabilities. Native debug and release builds check signed-to-`usize` conversion, region limits, and `no_gc` budgets at runtime; failed dynamic sizes report `CGR013` at the size argument. Region limits, budgets, object shapes, and reference indices stay literal-only. Package memory programs, cross-call handles, recursive functions, arbitrary control flow, foreign calls, and mixed scalar/memory bodies remain unsupported with source-located diagnostics. Failed checks, target emission, or compilation leave an existing requested output untouched, including `--force`.

To build the bounded source graph fixture from the repository root:

```sh
cargo run -p cgx2c --offline -- build crates/cgx2c/tests/fixtures/source_graph_cycle.cg --out /tmp/cgx2-source-graph --release
CGX2_MEMORY_REPORT=/tmp/cgx2-source-graph.json /tmp/cgx2-source-graph
```

Success exits 0 without output. A failed allocation exits 1 with `CGR013` and the source operation byte span; invalid graph references and other memory failures use `CGR014`. Manual regions collect only on explicit `memory.collect`, and `no_gc` suppresses collection with a cumulative allocation budget. Native builds use the same embedded runtime in debug and release; only the optimization level changes. The memory profile has a one-million-step execution limit and a 1 MiB auto collection trigger.

To capture the executing binary's memory counters, set the environment variable for that invocation:

```sh
CGX2_MEMORY_REPORT=/tmp/cgx2-memory-report.json /tmp/cgx2-memory-functions
```

The report is separate from stdout and contains `native_report_schema_version: 1`, `status`, `diagnostic_code`, and `memory` (the runtime's schema v1 report). Both successful and failed started programs write it. The default invocation does not write a report or change program arguments. The report is limited to 1 MiB. An invalid destination, symlink, directory, or the executable's own path fails before program execution with `CGR017`; an existing regular report is replaced atomically after execution. Invalid source, unsupported emission, or `rustc` failure cannot create a report and leave an existing binary untouched. Compare region and total committed, live, peak, allocation-failure, collection, and release (`total_collected_bytes`) counters with interpreter `MemoryReport::to_json()`; ignore pause timing and histogram values. The opt-in interpreter failure API now returns a partial `MemoryReport`, allowing code, source span, semantic counters, and rollback observation to be compared with native debug and release output. The parity test uses `cgx2c build --test-fail-reserve N`; ordinary builds have no active injection hook.

## Failure parity and host fault command

The checked fixture below fails after a successful reference mutation inside `main → dispatch → exhaust_after_link`. These direct commands exercise both native profiles:

```sh
cargo run --offline -p cgx2c -- build crates/cgx2c/tests/fixtures/failure_nested_mutation_limit.cg --out /tmp/cgx2-nested-debug
CGX2_MEMORY_REPORT=/tmp/cgx2-nested-debug.json /tmp/cgx2-nested-debug
cargo run --offline -p cgx2c -- build crates/cgx2c/tests/fixtures/failure_nested_mutation_limit.cg --out /tmp/cgx2-nested-release --release
CGX2_MEMORY_REPORT=/tmp/cgx2-nested-release.json /tmp/cgx2-nested-release
```

Both executions exit 1 with `CGR013 RegionLimit` on denied allocation. The operation snapshot has three rooted handles and one edge across two source-created regions; the final runtime census has zero registrations after unwind. `failure_report_parity.rs` compares diagnostic code and span, runtime census, semantic memory counters, and source-observed roots and edges between interpreter and native debug/release builds. It also covers collection, region close, reserve faults, stale handles, manual limits, and `no_gc` budgets. Cross-region edges and invalid reference slots are rejected by the checker (`CGM002`) before a binary or runtime snapshot exists.

`cgx2c build FILE.cg --out BINARY --test-fail-reserve N` makes an explicit test-only build, where `N` is the one-based runtime host reserve attempt to fail. It works with `--release` for parity tests. The option is rejected for `emit-rust`, package, and scalar builds; ordinary release binaries do not compile the injection feature and ignore `CGX2_TEST_FAIL_RESERVE`. For example:

```sh
cgx2c build crates/cgx2c/tests/fixtures/failure_host_fault.cg --out /tmp/cgx2-fault --test-fail-reserve 6
CGX2_MEMORY_REPORT=/tmp/cgx2-fault.json /tmp/cgx2-fault
```

The opt-in native report retains `native_report_schema_version: 1`, `failure_report_schema_version: 1`, `status`, `diagnostic_code`, `file`, `span`, `rollback_preserved`, and `memory`. Failure reports add `operation_failure`, `final_unwind`, and `cleanup_errors`. Both snapshots retain source-observed `rooted_handles` and `rooted_edges`, plus semantic per-region and total memory counters. They add `runtime_rooted_handles`, `runtime_root_registrations`, and `runtime_root_census_error` from `Session::root_census()`. A census failure names the error and leaves counts null. The first snapshot precedes local cleanup; the second follows all normal cleanup attempts. Failed unroots remain visible without replacing the primary diagnostic. The source path is embedded at build time; manually compiled `emit-rust` output reports `<source>` unless `CGX2_SOURCE_FILE` is set while compiling it. The report remains capped at 1 MiB and installed atomically. Static checker errors and unsupported native package graph programs produce no binary or runtime report.

**Root observation limit:** Zero final `runtime_root_registrations` is necessary evidence of released registered roots, not a comprehensive memory-safety proof. Legacy source-observed root and edge counts alone cannot establish cleanup.

The `tests/fixtures/reusable_memory_*.cg` files cover zero, negative, `i64::MAX`, region-limit, and `no_gc` budget cases. `tests/native_memory_reports.rs` and `tests/failure_report_parity.rs` compare interpreted and native debug/release memory counters, error codes, spans, and failure snapshots. On a 64-bit target `i64::MAX` fits `usize` and fails the region limit; narrower targets can fail signed-to-`usize` conversion first.
