# CGx2 package index preview

`cgx2-package::load_package(root)` reads `root/cgx2.toml`, checks listed `.cg` files, and returns a deterministic `PackageIndex`. It does not execute code, resolve imports, install dependencies, or infer a dependency graph. The index reports `imports_supported: false`.

## Manifest version 1 subset

All four keys are required, with no defaults:

```toml
schema_version = 1
name = "demo"
entry = "src/main.cg"
sources = ["src/main.cg", "src/math.cg"]
```

Each assignment occupies one line. Keys may appear in any order. Blank lines and whole-line `#` comments are allowed. Spaces around `=` and array commas are allowed. `schema_version` must be the unquoted integer `1`. `name` is 1–64 ASCII bytes: an initial letter followed by letters, digits, `_`, or `-`. `entry` and every `sources` item are one-line double-quoted UTF-8 basic strings. Only `\"`, `\\`, `\n`, `\r`, and `\t` escapes are implemented. Unsupported TOML syntax, including tables, dotted keys, literal strings, multiline strings and arrays, inline comments, trailing array commas, and other escapes, is rejected. `sources` has at most 128 entries. The manifest is at most 64 KiB, each source at most 1 MiB, and each listed path at most 240 bytes. `entry` must exactly match a listed source string.

Source paths must be relative `.cg` paths using `/`, without `.` or `..` components. The loader canonicalizes the package root and every listed path before reading source contents. It rejects paths and symlinks that resolve outside the root, and duplicate canonical files. A multi-file package requires a `module` declaration in each file; duplicate declared module names fail. A one-file script may omit `module`. All files are parsed and semantically checked, then `cgx2-api-index` supplies checked public function signatures. Modules sort by declared name, then path. Diagnostics contain a file path and a half-open UTF-8 byte span when source text is available.

Path case follows the host filesystem. On case-sensitive filesystems, differently cased names can denote different files; on case-insensitive filesystems they usually canonicalize to the same file and are rejected as duplicates. No case folding is performed by this crate. Package path strings use `/` in the manifest and output.

Run `cargo run -p cgx2-package --example show_index --offline` to print the JSON for [`fixtures/demo`](fixtures/demo). The committed [`expected-index.json`](fixtures/demo/expected-index.json) is checked by the integration test.
