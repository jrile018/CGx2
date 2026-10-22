# CGx2 Call and Effect Index Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, or create another chat.

**Goal:** Give humans and LLMs a deterministic, source-located index of function calls and conservative effects for one `.cg` module. This is analysis data for understanding code and later optimizer work, not a claim that the language already enforces effects.

**Architecture:** A new `cgx2-effect-index` library and CLI parse a `.cg` module, run existing semantic checks where supported, traverse the frozen syntax AST, build a local call graph, and compute transitive effects with a monotone fixed point. Serialize a stable versioned JSON schema and a concise human-readable text view. Unknown/dynamic/foreign calls always include `Unknown`; no analysis result may prove purity when a reachable unknown call exists.

**Tech stack:** Rust 1.98, `cgx2-syntax`, `cgx2-sema`, existing offline JSON dependencies if available.

**Specification:** `crates/cgx2-syntax/src/ast.rs`, `crates/cgx2-sema/src/lib.rs`, `docs/language-design.md`, `plans/wave6/README.md`.

## Owned paths and fixed boundaries

- Own only new `crates/cgx2-effect-index/**` and this plan. Do not edit parser, AST, sema, CLI, runtime, package index, root manifests/lock, or docs. Coordinator registers the crate after review.
- Command: `cgx2-effect-index FILE.cg --format text|json` (`text` default). Exit nonzero on parse/semantic failure with code, message, and source span. Use an explicit schema version and stable sorting for functions, calls, and effects.
- Index each local function name, declaration span, public flag, parameter/return type text when available, direct calls with call-site spans, and direct/transitive effect sets. Include a module-level entry for top-level statements. Report recursion/cycles explicitly; complete fixed-point propagation must terminate.
- Use a small named effect vocabulary: `ReadArgs`, `ManagedAlloc`, `Collect`, `Mutate`, `Foreign`, `Unknown`. Classify only syntax and known builtins with a documented exact mapping; effects for unknown calls and unresolved names are `Unknown`, even if the target name looks harmless. `memory.*` operations use the frozen Wave 5 names; allocations and collection cannot be marked pure. A local call inherits callee effects transitively. Side-effect-free scalar expressions with fully resolved local calls may have an empty effect set, but this does not grant parallelization rights.
- Keep call-target classification conservative. No dynamic dispatch inference, whole-program import resolution, parallel or GPU safety recommendation, capability enforcement, or source effect annotations. If sema cannot check a currently supported memory script, expose a clearly named syntax-only mode and mark unresolved facts `Unknown`; do not silently skip checks.
- Spans must use the parser's byte offsets and report their coordinate system. JSON must be machine-readable, deterministic, and bounded for a module within parser limits. Avoid string-scraping source to find calls.

## Review focus

1. A pure local function chain has correct direct/transitive calls and empty effects; an unknown or foreign call propagates `Unknown` to all callers.
2. Memory allocation/collection and mutation are conservatively classified, and top-level effects are included.
3. Mutual recursion terminates and yields stable results. Repeated runs produce byte-identical JSON for the same source.
4. Diagnostics distinguish parse/check errors from unresolved analysis facts; no output claims GPU or parallel safety.

## Tasks

### 1. Schema and traversal

- [ ] Define a versioned JSON schema and exact effect mapping in crate README, including byte-offset spans and how syntax-only fallback is requested.
- [ ] Add failing tests for nested calls, top-level statements, memory builtins, unknown calls, mutation, recursion, and deterministic order.
- [ ] Implement AST visitor for expressions and statements; resolve only same-module named functions and exact known builtins. Compute effect closure to convergence.

### 2. Command and diagnostics

- [ ] Implement text/JSON command with bounded source read and errors that contain source location. Include small `.cg` examples and expected output in README.
- [ ] Add a property-style or generated test for larger cyclic call graphs to ensure termination and monotonicity.

### 3. Verification and handoff

- [ ] Run `cargo test -p cgx2-effect-index --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-effect-index --all-targets --offline -- -D warnings` where the standalone package permits.
- [ ] Commit only owned paths and report representative JSON, limitations, and any semantic-check fallback used.

**Acceptance:** A deterministic conservative call/effect index exists for supported source, with `Unknown` propagation and no misleading purity or optimization claims.
