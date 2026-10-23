# CGx2 contract call graph completion

## Goal

Include calls made only inside requires/ensures in published function and package call graphs.

Read the parent review README.md, master completion plan, frozen contracts, common worker rules, design/validation plan and local AGENTS.md. This is an authorized review repair, not Stage 2.

## Exclusive write ownership

- `crates/cgx2-effect-index/**`
- `crates/cgx2-security/**`

Root Cargo files, all shared plans and every other lane are read-only. Preserve frozen interfaces consumed by the other two repair chats. Use private target/TMPDIR and CARGO_BUILD_JOBS=2. If /tmp quota still prevents sandbox startup, report it accurately and use the normal approval mechanism for scoped commands with disk-backed TMPDIR; do not delete unrelated temporary data.

## Required work and acceptance

Review found cgx2-effect-index/src/lib.rs function indexing visits function.body only. The new package_v1 analyzer consumes that index, so a helper used exclusively in a contract is absent. No taint-policy bypass has been demonstrated; this is a concrete graph completeness gap.

Traverse requires and ensures under the correct lexical scopes: parameters visible in preconditions, result visible in postconditions, body locals excluded from contracts, ordinary shadowing and same-named helpers resolved correctly. Preserve deterministic source spans, direct/local/qualified call classification and conservative unresolved behavior. Exercise public checked package summaries and standalone syntax-only behavior; do not weaken checker purity or import trust rules. Ensure contract calls participate in derived graph propagation without double counting edges/effects or fabricating foreign capabilities. Keep JSON/SARIF/provenance behavior compatible.

Add contract-only same-module helper tests, duplicate call spans, parameter/result shadowing, call in nested contract arguments/branches and multibyte spans. Tests should check actual graph edges and relevant package output. Run both owned package suites, Clippy/fmt and regression corpus. Record no general security proof; commit the owned implementation/evidence and stop.

## Stop condition

Record exact commands/results, limitations, files and commit. Update MegaMemory and stop for coordinator review. No merge, delegation, additional chats, unrelated feature work or external installation.
