# CGx2 Local Package Import Plan

> Implement this plan in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Make multi-file `.cg` packages statically meaningful: explicit imports resolve to listed local modules, imported public scalar functions type-check, and missing/private/cyclic dependencies fail with source locations.

**Architecture:** Add top-level `import app.math` declarations to the syntax AST and formatter. `cgx2-package::load_package` reads all manifest-listed sources, builds an acyclic declared-module graph, collects explicitly annotated public signatures, and checks each module with a new sema entry point given an imported-signature environment. Preserve the existing single-file `check_module` API. Expose imports and graph order in the package index and a `cgx2-package check ROOT [--format text|json]` command. No package execution or cross-module code generation yet.

**Tech stack:** Rust 1.98, existing syntax/sema/package/API-index crates, no network dependency resolution.

**Specification:** `docs/language-design.md` modules, `crates/cgx2-package/README.md` manifest v1, `plans/wave7/README.md` interface contract.

## Owned paths and exact source rules

- Own only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-package/**`, `crates/cgx2-api-index/**`, and this plan. Do not modify CLI, native compiler/emitters, effect index, memory runtime, root files, or other lanes. Keep existing public `parse_module`, `check_module`, `load_package` and scalar/memory behavior compatible. If an unchanged downstream crate needs an edit, report the exact compile error to the coordinator rather than crossing the ownership boundary.
- Grammar: optional `module dotted.name`, then zero or more `import dotted.name` lines, then functions or a script. Imports occur only at top level before executable items, are unique, and have source spans. No glob import, alias, relative path, external package download, or runtime initialization. The formatter preserves canonical four-space indentation and one import per line with deterministic order only if reordering is semantics-preserving; otherwise preserve source order.
- Each import must name exactly one declared module among manifest `sources`. Reject missing modules, duplicate declarations/imports, self-imports, cycles with a stable path trace, and source paths outside the manifest root. The entry module and every listed source are checked; unlisted files are never loaded implicitly. Reuse existing canonical path protection.
- `app.math.double(3)` resolves only when `app.math` was explicitly imported and `double` is public. Imported public functions need explicit parameter/return annotations in this slice. Check arity and scalar argument/return types using the same diagnostics as local functions, with a related declaration location when practical. A private or unimported function is a distinct diagnostic from a missing package module. Imports of memory-script modules or calling memory intrinsics through an import are unsupported with a clear diagnostic.
- The package index reports each module's imports, resolved public calls, and a deterministic topological order with schema version/field changes documented. Existing one-file packages without imports still check. `imports_supported` becomes true only for this bounded static local profile; docs must say imported code cannot yet run via `cgx2 run` or `cgx2c build`.

## Tasks

### Syntax and static signatures

- [ ] Add red lexer/parser/formatter tests for valid import placement, UTF-8 byte spans, duplicates, invalid nesting, and round-trip formatting.
- [ ] Add AST import declarations without changing existing statement variants. Keep downstream source pattern matches compiling; run workspace compile early.
- [ ] Add a sema API for checking a module with explicitly supplied imported public scalar signatures. Keep `check_module` unchanged for standalone files and unknown import calls.

### Package graph

- [ ] Add fixture packages for valid two/three-module calls, missing/private/unimported symbols, cycles, duplicate names, wrong type/arity, and a symlink/path escape. Assert stable file/spans and graph order.
- [ ] Implement multi-pass signature collection, graph resolution, typed calls, deterministic JSON/text index, and `cgx2-package check ROOT` with nonzero errors. Avoid executing imported code or invoking a compiler backend.
- [ ] Update owned crate READMEs with exact scope and machine-readable example.

### Verification and handoff

- [ ] Run `cargo test --workspace --exclude cgx2-python-bridge --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy --workspace --exclude cgx2-python-bridge --all-targets --offline -- -D warnings`. If local HTTP socket tests need permission, run them with appropriate sandbox access or report that exact environmental limit.
- [ ] Commit only owned paths and report graph/signature examples, downstream compatibility, and remaining execution gap.

**Acceptance:** A declared local import gives a source-located, acyclic, typed package graph and permits public scalar calls to check; unsupported execution fails or remains unavailable clearly.
