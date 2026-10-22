# CGx2 Public API Index Implementation Plan

> **For agentic workers:** Implement this plan task by task in your attached worktree. Do not delegate or edit another lane. Commit the bounded deliverable and stop for user review.

**Goal:** Give humans and LLMs a deterministic, queryable index of public CGx2 function signatures and source locations.

**Architecture:** A new `cgx2-api-index` library pairs the frozen syntax `Module` with a successful `TypedModule`, excludes private functions and preview builtins, and produces stable text and JSON output. It is a per-module index now; package import graphs and effect/contract summaries will be added when syntax and typed IR support them.

**Tech Stack:** Rust 1.98, `cgx2-syntax` and `cgx2-sema` path dependencies, standard library only.

**Spec:** `docs/language-design.md` module/API index and LLM tooling sections; `plans/wave3/README.md` frozen inputs.

## Global Constraints

- Own only `crates/cgx2-api-index/**` and this plan file. Do not edit parser, semantic checker, CLI, root manifests, or shared docs.
- Index only source-declared `pub fn` items with successful type checking. Builtins with synthetic spans and private functions are excluded. Preserve parameter names/order, resolved type names, return type, module name if declared, and declaration byte span.
- Sort functions deterministically by name, then declaration byte span. `find(name)` is exact; missing symbol is distinct from a missing module or invalid typed summary.
- JSON output is UTF-8 and valid for quotes, backslashes, control characters, and Unicode names. The schema has `schema_version: 1`, `module`, and `functions`; each function has `name`, `parameters`, `return_type`, and `span` `{start,end}`.
- Do not invent effects, capabilities, contracts, documentation comments, or dependency edges that are absent from the current AST. A `cgx2 api` CLI command belongs to a later integration pass.

## Review Focus

1. A private function or builtin must never appear in text or JSON output; test in Task 1.
2. An index built with a `TypedModule` from another AST must fail rather than attach wrong types; test in Task 2.
3. Unicode names and source offsets must remain byte-accurate; test in Task 2.
4. JSON with escaped unusual names or module path must parse and retain original values; test in Task 3.
5. Empty public API is a valid index and renders deterministically; test in Task 3.

## Files and public contract

- Create `crates/cgx2-api-index/Cargo.toml`, `src/lib.rs`, `src/index.rs`, `src/render.rs`, `src/error.rs`, `tests/index.rs`, and `README.md`.
- Export `ApiIndex { schema_version: u32, module: Option<String>, functions: Vec<ApiFunction> }`, `ApiFunction { name: String, parameters: Vec<ApiParameter>, return_type: String, span: Span }`, `ApiParameter { name: String, ty: String }`, `IndexError`, and `build_index(module: &cgx2_syntax::Module, typed: &cgx2_sema::TypedModule) -> Result<ApiIndex, IndexError>`.
- Export `ApiIndex::find(&self, name: &str) -> Option<&ApiFunction>`, `render_json(&self) -> String`, and `render_text(&self) -> String`. Schema version defaults to 1; no network or filesystem access occurs in the library.

### Task 1: Public signature extraction

- [ ] Add failing tests for module path, public/private functions, inferred private return, parameter ordering/types, and builtin exclusion.
- [ ] Run `cargo test -p cgx2-api-index --offline` and record failures.
- [ ] Build the index from public AST functions and matching typed signatures; preserve byte spans and deterministic order.
- [ ] Re-run tests and verify Task 1 cases pass.

### Task 2: Consistency and lookup

- [ ] Add failing tests for a stale or mismatched typed summary, exact lookup success/miss, UTF-8 spans, and empty index.
- [ ] Run them red, implement consistency checks and `find`, then rerun. Return an explicit `IndexError` instead of panicking or silently dropping a public function.

### Task 3: Human and machine output

- [ ] Add failing snapshot/round-trip tests for stable text order, JSON schema fields, escaped names/path, and empty public API. Parse JSON in tests using a bounded helper or Python stdlib; do not assert only substrings.
- [ ] Run red, implement renderers, then run `cargo test -p cgx2-api-index --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --check --manifest-path crates/cgx2-api-index/Cargo.toml`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-api-index --all-targets --offline -- -D warnings`.
- [ ] Document schema and limitations, especially absent effects/contracts/imports. Commit only owned paths and stop.

**Acceptance:** valid public signatures have exact, stable text/JSON representations and failed/mismatched inputs are explicit. This is a per-module library index, not a complete package graph or CLI command.
