# Native compilation of checked source graphs

Use the attached worktree; read shared Wave 14 references and this plan. Commit only owned paths and stop for review.

**Goal.** Compile the exact bounded `.cg` managed-reference grammar already accepted by Wave 13 checking and interpretation. The compiler must either run the same memory semantics or issue a source-located unsupported-target diagnostic. It must never silently accept graph code and emit a scalar-only program.

**Ownership.** Only `crates/cgx2-emit-rust/**` and `crates/cgx2c/**`. Read the frozen Wave 13 source checker and runtime API. Do not edit them, root Cargo files, plans, or other lanes. Preserve the existing scalar and local memory-function output. Embedded runtime sources must be complete, including `fault.rs`.

## Deliverable

1. Translate the frozen source subset for local object allocation, bounded literal reference slots, set/get/clear edges, rooting, collection, region close, and nested `no_gc` where the checker accepts them. Use runtime methods rather than duplicating collector logic in generated code. Retain source spans for every rejected unsupported form.
2. For successful programs, compare interpreter and debug/release native output plus structured memory report on live bytes, releases, and actual collection count. Include a parent-child cycle, dangling-edge rejection, manual limit failure, and an injected-failure case. If interpreter errors expose no partial report, compare error class and state only and document this exact missing parity surface.
3. Ensure `cgx2c build`, direct emitted Rust, and package-entry behavior reject unsupported graph calls consistently. No claim of arbitrary typed objects, concurrency, or foreign-handle safety.
4. Update owned README with accepted syntax shape, target limitations, build invocation, and report comparison method.

## Verification

Run `cargo test -p cgx2-emit-rust -p cgx2c --offline`, format, targeted Clippy with warnings denied, and compile/run at least one accepted and one rejected `.cg` fixture in debug and release. Use the frozen baseline and name any source grammar that remains unsupported. Report command output, parity table, and commit SHA.

**Acceptance.** Checked supported source graph code has a native path with the same observed memory outcomes; unsupported code fails explicitly before executing generated output.
