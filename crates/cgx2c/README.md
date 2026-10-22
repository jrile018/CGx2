# cgx2c scalar native compiler preview

From the repository root, with Rust 1.98 and `rustc` installed:

```sh
cargo run -p cgx2c --offline -- build crates/cgx2c/examples/hello.cg --out /tmp/cgx2-hello
/tmp/cgx2-hello
cargo run -p cgx2c --offline -- emit-rust crates/cgx2c/examples/hello.cg --out /tmp/cgx2-hello.rs
rustc --edition=2024 /tmp/cgx2-hello.rs -o /tmp/cgx2-hello-manual
```

If the emitted `.rs` filename contains spaces or punctuation, add `--crate-name cgx2_generated` to the manual `rustc` command. `cgx2c build` uses a fixed temporary Rust filename and accepts such output paths directly.

`build FILE.cg --out BINARY [--release] [--force] [--rustc PATH]` checks and emits the scalar profile, then invokes `rustc` with argument vectors. `--release` adds `-O`. The default compiler is `rustc` on `PATH`; `CGX2C_RUSTC` or `--rustc PATH` selects another executable. `emit-rust FILE.cg --out FILE.rs [--force]` writes the deterministic, readable emitter output. Both commands require an explicit output. Existing output requires `--force`; an output that is the input file and existing symlink outputs are rejected. Source is limited to 4 MiB and emitted Rust to 32 MiB.

Generated files and binaries are staged in a private directory alongside the requested output and removed on error. On Unix, installation without `--force` uses an atomic hard link that cannot replace an existing output; with `--force`, rename atomically replaces a regular file. The preceding symlink check is best effort against concurrent path changes. Filesystems without hard link support cannot use the no-force installation. Windows replacement behavior depends on its rename semantics; this preview is verified on Unix.

After a successful build, stdout contains a build metadata receipt with SHA-256 of the source bytes, selected `rustc` path, `rustc -vV` version and host target, profile, and output path. Capture that line if persistent provenance is needed. Version and target say `unavailable` if the selected compiler cannot answer `-vV`; a wrapper may report its own identity. This preview does not record linker flags, external environment, or a reproducible build attestation.

Source diagnostics retain the syntax (`CGS`), type (`CGT`/`CGM`/`CGO`), or Rust target (`CGRS`) code and UTF-8 byte span. `CGXC001` is command usage, `CGXC002` source I/O, `CGXC004` an unsafe or occupied output, `CGXC005` a `rustc` failure, and `CGXC006` staging/install I/O. Compiler stderr is capped at 8 KiB and control characters are sanitized.

The compiler accepts only the checked scalar Rust-emitter profile. Memory scripts, loops, foreign calls, Python/PyTorch, package imports, effects, and recursion do not compile through this route. This is not full native CGx2, and a successful build makes no performance claim.
