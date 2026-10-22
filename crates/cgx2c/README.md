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

The compiler accepts checked scalar and bounded memory Rust-emitter profiles. Memory function modules need `main()` and permit direct zero-argument local calls, literal-size memory operations, including `alloc_object`, `set_reference`, `clear_reference`, `reference`, and `use_reference`, and nested `no_gc`; calls share one runtime session. The checker caps object payloads at 1 MiB and reference slots at 256. Package imports work only through `--package`; package memory programs remain unsupported and graph calls produce a source-located `CGRS001` target diagnostic. Scalar `requires`/`ensures` execute the supported expression subset; unsupported pure calls in contracts receive source-located target errors. Declared effects and capabilities must pass the existing static checker. Cross-call memory handles, dynamic sizes or calls, recursive memory functions, loops, foreign calls, Python/PyTorch, arbitrary typed objects, concurrency, and foreign-handle safety remain outside this target profile. Failed checks, target emission, or compilation leave an existing requested output untouched, including with `--force`.

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

The report is separate from stdout and contains `native_report_schema_version: 1`, `status`, `diagnostic_code`, and `memory` (the runtime's schema v1 report). Both successful and failed started programs write it. The default invocation does not write a report or change program arguments. The report is limited to 1 MiB. An invalid destination, symlink, directory, or the executable's own path fails before program execution with `CGR017`; an existing regular report is replaced atomically after execution. Invalid source, unsupported emission, or `rustc` failure cannot create a report and leave an existing binary untouched. Compare region and total committed, live, peak, allocation-failure, collection, and release (`total_collected_bytes`) counters with interpreter `MemoryReport::to_json()`; ignore pause timing and histogram values. The current interpreter API returns no partial report on execution error, so failure tests compare error code and source span while inspecting native state counters. A conformance test compiles emitted Rust with the frozen runtime's `fault-injection` feature and injects a host reserve failure at `alloc_object`; ordinary builds have no injection hook.
