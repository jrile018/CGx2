# CGx2 package index preview

`cgx2-package::load_package(root)` reads `root/cgx2.toml`, checks exactly the listed `.cg` files, and returns a deterministic static `PackageIndex`. It resolves explicit local imports, rejects cycles, and type-checks imported public scalar calls. `cgx2c build --package ROOT` executes the bounded scalar subset; `cgx2 run` remains standalone.

`load_checked_package(root)` returns a read-only `CheckedPackage` with `index()`, `root()`, `manifest_sha256()`, and `modules()`. Each `CheckedModule` exposes its manifest-relative `source()`, `canonical_path()`, `source_sha256()`, checked `text()`, `syntax()`, and `typed()`. The loader reads each source once for parsing and typing. Call `verify_unchanged()` before publishing a generated artifact; it rechecks manifest/source SHA-256 and canonical source targets and reports `CGP018` on changed bytes or targets. The compiler checks again after `rustc` and before installation. Hashes identify the validated inputs, not a reproducible build attestation.

## Manifest version 1 subset

All four keys are required, with no defaults:

```toml
schema_version = 1
name = "demo"
entry = "src/main.cg"
sources = ["src/main.cg", "src/math.cg"]
```

Each assignment occupies one line. Keys may appear in any order. Blank lines and whole-line `#` comments are allowed. Spaces around `=` and array commas are allowed. `schema_version` must be the unquoted integer `1`. `name` is 1–64 ASCII bytes: an initial letter followed by letters, digits, `_`, or `-`. `entry` and every `sources` item are one-line double-quoted UTF-8 basic strings. Only `\"`, `\\`, `\n`, `\r`, and `\t` escapes are implemented. Unsupported TOML syntax, including tables, dotted keys, literal strings, multiline strings and arrays, inline comments, trailing array commas, and other escapes, is rejected. `sources` has at most 128 entries. The manifest is at most 64 KiB, each source at most 1 MiB, and each listed path at most 240 bytes. `entry` must exactly match a listed source string.

Source paths must be relative `.cg` paths using `/`, without `.` or `..` components. The loader canonicalizes the package root and every listed path before reading source contents. It rejects paths and symlinks that resolve outside the root, and duplicate canonical files. A multi-file package requires a `module` declaration in each file; duplicate declared module names fail. A one-file script may omit `module`. Each `import app.math` must name exactly one listed declared module, precede executable items, and be unique. Self-imports and cycles fail with a stable path trace. Only a public function such as `app.math.double(3)` can be called, and only after `import app.math`. Its parameter and return annotations must use the checked scalar types `i64`, `Bool`, `String`, or `Unit`; `List<T>` import calls are outside this slice. Memory-script modules and `memory.*` calls through imports are unsupported. Modules sort by declared name, then path. Diagnostics contain a file path and a half-open UTF-8 byte span when source text is available. Type and arity errors on imported calls include a related declaration location.

Path case follows the host filesystem. On case-sensitive filesystems, differently cased names can denote different files; on case-insensitive filesystems they usually canonicalize to the same file and are rejected as duplicates. No case folding is performed by this crate. Package path strings use `/` in the manifest and output.

Run `cargo run -p cgx2-package --example show_index --offline` to print the JSON for [`fixtures/demo`](fixtures/demo). The committed [`expected-index.json`](fixtures/demo/expected-index.json) is checked by the integration test.

Run `cargo run -p cgx2-package --bin cgx2-package --offline -- check crates/cgx2-package/fixtures/imports --format json` for the three-module fixture. `--format text` is the default. Invalid packages exit with status 1 and emit source-located diagnostics; malformed invocation exits with status 2. JSON errors are one object per line. For example:

```json
{"code":"CGP014","file":"src/main.cg","message":"imported module `app.absent` is not listed in this package","span":{"start":23,"end":33}}
```

## Package index schema version 2

The package index has `schema_version: 2` and `imports_supported: true` for this bounded static profile. Each sorted module record adds `imports` (source order) and `resolved_public_calls` (source order, with zero-based half-open call spans). The top-level `topological_order` is dependency-first with lexical tie breaking. The per-module API index remains schema version 1. The committed demo index gives a complete machine-readable example.
