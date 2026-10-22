# CGx2 Package Manifest and Module Index Plan

> Implement this plan in your attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Make a small `cgx2.toml` package discoverable and inspectable without pretending source imports or dependency installation already work.

**Architecture:** Add a `cgx2-package` library that parses a deliberately strict version-1 manifest, resolves listed `.cg` source paths under a package root, parses/checks each module using existing syntax/sema, and builds a deterministic index of declared module names and public API entries using `cgx2-api-index`. Reject duplicate modules, missing entry, path escape, symlink escape, invalid UTF-8, and unknown manifest keys. The output is an index of known modules with no import edges until source import syntax is implemented.

**Tech stack:** Rust 1.98, existing syntax/sema/API-index crates, standard library, offline dependencies only. No registry/network resolution.

**Specification:** `docs/language-design.md` module/manifest section, `plans/wave5/README.md`.

## Owned paths and fixed boundaries

- Own only new `crates/cgx2-package/**` and this plan. Do not edit parser, sema, API index, root manifests/lock, CLI, or shared docs.
- Define and document an exact strict subset of `cgx2.toml`, for example `schema_version = 1`, `name = "example"`, `entry = "src/main.cg"`, and `sources = ["src/main.cg", "src/math.cg"]`. Follow TOML quoting/escaping only to the extent implemented; reject rather than misparse unsupported TOML. It is a first manifest subset, not a complete TOML implementation.
- `load_package(root: &Path) -> Result<PackageIndex, Vec<PackageDiagnostic>>` (or comparable) returns sorted source/module records, entry path, public API summaries, and source-located errors. File errors must name the file and path; manifest errors must include line/column or byte span.
- All sources must stay inside the canonical package root, including symlinks. Reject duplicate paths after canonicalization and duplicate declared `module` paths. A source lacking a module declaration is an error in a multi-file package. `entry` must be one listed source. Do not execute code or import Python packages.
- The source parser currently has no `import` statement. The index must expose `imports_supported: false` or equivalent and have no invented dependency edges. A full package graph, registry, and version solver follow later.

## Review focus

1. A two-module package returns deterministic module order, public signatures, and exact entry. Running twice produces identical text/JSON index.
2. Duplicate module names, duplicate source paths, missing entry, unknown keys, malformed strings, and invalid source diagnostics fail explicitly.
3. `../` and symlinks outside the root are rejected before reading external source content into the package index.
4. Path case behavior is documented for the host; an import graph is not fabricated from source names.
5. The public API index remains correct for scalar modules and does not imply effects/contracts that the frontend has not checked.

## Tasks

### 1. Manifest grammar and path safety

- [ ] Add red tests for valid manifest, duplicates, unknown keys, missing fields, invalid UTF-8, `../` escape, and outside symlink. Use temporary directories and no network.
- [ ] Implement strict parser with fixed limits on manifest/source count/bytes. Canonicalize and validate paths under root before source reads.
- [ ] Document exact key names, defaults (if any), and limits in README; reject unsupported TOML features.

### 2. Module/API index

- [ ] Add red tests for two-module package, duplicate module declarations, source semantic errors, missing public signatures, and stable render order.
- [ ] Parse/check each `.cg`, call existing API-index library, and build deterministic records. Include source paths/spans in diagnostics.
- [ ] Expose text/JSON rendering with schema version and explicit unsupported-import field; no invented dependency resolution.

### 3. Verification and handoff

- [ ] Run `cargo test -p cgx2-package --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-package --all-targets --offline -- -D warnings`.
- [ ] Commit only owned paths and include a small example package with its expected index in the crate fixtures.

**Acceptance:** A safe local package index works for declared module files and public signatures; imports, dependency fetching, and package execution remain explicit next steps.
