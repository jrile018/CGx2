# Checked managed references in `.cg` source

> Use the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Give the Wave 12 direct-runtime managed graph a small checked `.cg` source path. Memory correctness is the priority: an unsupported handle shape or escape must fail at `cgx2 check`, not become an unchecked runtime call.

**Architecture:** Extend the existing memory-operation checker and evaluator with explicit local operations corresponding to frozen `Session::alloc_object(region, payload_len, reference_slots)`, `set_reference(owner, index, target)` and `reference(owner, index)`. Use one `Session` per program, source spans, existing region and `no_gc` rules, and existing handle ownership facts. Choose and fixture-test one readable source spelling; do not introduce a second general heap or implicit cross-region reference. Restrict slots to literal bounded indices and object shapes to literal bounded sizes where the current checker can prove them. Return an optional reference only through a checked local operation; no cross-function handle transfer, global storage, closures, foreign calls or raw pointers.

**Ownership:** Only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, and `crates/cgx2-api-index/**`. Read runtime/emitters but do not edit them, root manifests/lock, docs or plans. Consume the frozen Wave 12 graph API; the parallel runtime lane preserves it. `cgx2c` and Python emission may reject new graph source with explicit target diagnostics, but this lane cannot edit those emitters. If their frozen checks would silently accept and miscompile, stop at the smallest safe source subset and report the integration dependency.

## Required behavior

1. Add a source fixture that allocates two objects in one manual region, roots the parent, links child through one checked slot, collects, reads the live child, clears the slot, collects again, and closes the region. Include a cycle fixture and a nested `no_gc` fixture; compare checked source behavior and per-region report to equivalent direct-runtime operations.
2. Negative fixtures: out-of-range slot, stale/released handle, wrong owner kind, cross-region edge, escaping handle, dynamic shape/index, use after region close, `no_gc` budget exhaustion, and hidden collect. Static impossibilities fail with source-located diagnostics; dynamic failures keep the runtime's error type, leave prior valid state intact, and never emit success output.
3. `cgx2 check`, `run`, JSON diagnostics, formatter and API index agree on the accepted profile. Preserve scalar, Python-only and explicit unsafe native C behavior. Do not claim typed records, concurrent collection, foreign tensor memory or native graph compilation.
4. Update owned READMEs with grammar, parameter limits, rooting/edge semantics and explicit unsupported forms.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-syntax -p cgx2-sema -p cgx2-cli -p cgx2-api-index --offline`, format and targeted Clippy `-D warnings`, plus direct check/run success and rejection examples. Commit only owned paths; report SHA, exact grammar, parity/report cases, and deferred target support.

**Acceptance:** Safe source can manipulate a bounded same-region object graph with the frozen runtime guarantees, and unmodeled escape or cross-region aliasing is rejected.
