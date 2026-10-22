# CGx2 Native Notebook Format Implementation Plan

> **For agentic workers:** Use the executing-plans skill task by task. This chat owns one bounded deliverable; do not spawn another agent, edit shared files, merge, or start a second assignment. Stop for user review after verification.

**Goal:** Implement a readable, versioned `.cgnb` notebook document and rich-asset storage format that round-trips safely without a CGx2 compiler.

**Architecture:** A line-oriented parser recognizes notebook version and column-zero cell/output/asset headers, preserving code and Markdown bodies exactly. Stable cell IDs and output records are typed data. Binary assets are stored by SHA-256 in a sibling `.cgnb.assets` directory; a self-contained export embeds their base64 bytes and verifies hashes on import.

**Tech Stack:** Rust stable 1.98.1; pinned `sha2 = 0.10.9`, `base64 = 0.22.1` if offline resolution succeeds. No UI framework or compiler dependency.

**Spec:** `docs/language-design.md` notebook section and `docs/validation-plan.md` notebook acceptance.

## Global constraints

- Own only `crates/cgx2-notebook-format/**` and `plans/notebook/plan.md`. Do not implement a Jupyter `.ipynb` format or a live kernel in this slice.
- Notebook header is `#!cgnb 1`. Cell headers at column zero are `%% code id=<id>` and `%% markdown id=<id>`. IDs are unique ASCII letters/digits/hyphens/underscores, 1–64 bytes.
- A line that would otherwise begin `%%` in cell/output content is escaped as `\\%%`; parse removes one leading backslash, writer adds it back. All other body bytes are preserved, including trailing newline behavior.
- Text outputs have a cell ID, MIME type, execution ordinal, and UTF-8 body. Binary outputs reference a SHA-256 digest and MIME type; the sidecar filename is the lowercase digest, never a user-provided path.
- Readers reject unsupported version, duplicate IDs, malformed headers, missing assets, hash mismatches, invalid UTF-8 in the notebook, and size-limit violations with structured errors.
- Saved output is cache/provenance; this crate does not declare execution correctness or dependency freshness.

## Public contract

Create `crates/cgx2-notebook-format/Cargo.toml`, `src/lib.rs`, `src/parse.rs`, `src/write.rs`, `src/assets.rs`, `tests/roundtrip.rs`, `tests/assets.rs`, `tests/fixtures/`, and `README.md`.

- `Notebook { version: u32, cells: Vec<Cell> }`; `Cell { id: String, kind: CellKind, source: String, outputs: Vec<Output> }`; `CellKind::{Code, Markdown}`; `Output::{Text { mime, ordinal, body }, Asset { mime, ordinal, sha256 } }`.
- `parse_notebook(text: &str) -> Result<Notebook, NotebookError>`.
- `write_notebook(notebook: &Notebook) -> Result<String, NotebookError>` produces canonical headers/order and round-trips sources.
- `save_asset(root: &Path, bytes: &[u8]) -> Result<String, NotebookError>` returns lowercase SHA-256 digest; `load_asset(root: &Path, digest: &str) -> Result<Vec<u8>, NotebookError>` verifies it.
- `export_self_contained(notebook: &Notebook, asset_root: &Path) -> Result<String, NotebookError>` writes embedded base64 asset blocks; `import_self_contained(text: &str, asset_root: &Path) -> Result<Notebook, NotebookError>` validates hashes and restores assets.

## Review focus

1. A literal `%%` inside code must survive parse/write/parse without becoming a new cell.
2. Duplicate cell IDs and outputs referencing nonexistent cells must fail clearly.
3. An asset digest may never be used as an unchecked path; `../` and uppercase/invalid digests fail.
4. Corrupted sidecar bytes must be caught by hash verification.
5. Export/import must work with no sidecar directory present at the destination.

## Tasks

### T1: Text grammar and round-trip

- depends_on: []
- owns: [`crates/cgx2-notebook-format/**`]
- non_goals: [execution session, UI, `.ipynb`]
- consumes: [notebook format proposal]
- produces: [typed `Notebook`, parser, canonical writer]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Create fixtures for one Markdown and two code cells, escaped leading `%%`, text output, duplicate ID, bad version, and truncated header.
- [ ] Add failing tests for `parse_notebook` and `write_notebook`; run `cargo test --manifest-path crates/cgx2-notebook-format/Cargo.toml --offline` to confirm missing behavior.
- [ ] Implement the parser/writer, preserving source body bytes and canonicalizing headers.
- [ ] Re-run tests; every valid fixture round-trips and every invalid fixture yields a typed error.

### T2: Content-addressed assets and self-contained export

- depends_on: [T1]
- owns: [`crates/cgx2-notebook-format/**`]
- non_goals: [plot rendering, HTML execution]
- consumes: [notebook/output model]
- produces: [asset store, embedded export/import]
- estimate: M
- coordination_risk: low
- status: pending

- [ ] Add failing tests for PNG bytes, duplicate-content deduplication, digest validation, corrupt bytes, missing asset, and export/import into an empty temporary directory.
- [ ] Run the crate tests to observe missing behavior.
- [ ] Implement SHA-256 filenames and base64 embedded blocks with byte-size limits. Reject path traversal and mismatched hashes before writing assets.
- [ ] Run the crate tests; export reopens without the original sidecar.

### T3: Validation, documentation, and review handoff

- depends_on: [T2]
- owns: [`crates/cgx2-notebook-format/**`, `plans/notebook/plan.md`]
- non_goals: [calling the CGx2 compiler, stale-cell dependency graph]
- consumes: [working parser and asset store]
- produces: [format README and verified standalone crate]
- estimate: S
- coordination_risk: low
- status: pending

- [ ] Run `cargo fmt --manifest-path crates/cgx2-notebook-format/Cargo.toml --check`, `cargo clippy --manifest-path crates/cgx2-notebook-format/Cargo.toml --offline -- -D warnings`, and crate tests.
- [ ] Document grammar, escaping, limits, sidecar naming, self-contained export, and the unimplemented live-session features.
- [ ] Commit owned paths and stop. Report tests, commit ID, offline dependency status, and format decisions needing user review.

**Acceptance:** all text and asset fixtures pass; the format is readable and versioned; output references cannot escape the asset directory; clean export/import succeeds. No live notebook behavior is claimed.
