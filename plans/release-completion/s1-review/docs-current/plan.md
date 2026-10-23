# CGx2 integrated command guide

## Goal

Update the executable release guide from Wave 28 command behavior to the integrated Stage 1 command surface.

Read the parent review README.md, master completion plan, frozen contracts, common worker rules, design/validation plan and local AGENTS.md. This is an authorized review repair, not Stage 2.

## Exclusive write ownership

- `docs/first-release/**`
- `tools/docs-check/**`

Root Cargo files, all shared plans and every other lane are read-only. Preserve frozen interfaces consumed by the other two repair chats. Use private target/TMPDIR and CARGO_BUILD_JOBS=2. If /tmp quota still prevents sandbox startup, report it accurately and use the normal approval mechanism for scoped commands with disk-backed TMPDIR; do not delete unrelated temporary data.

## Required work and acceptance

The integrated checker reports 26 passing, 2 failing, 6 blocked and 8 future cases. Failures are unified-build-unsupported and unified-emit-unsupported: core now supports these commands with an explicit --out destination. Old expected CGT-style unknown-command results no longer apply; actual missing-output diagnostic is CGXC001.

Replace obsolete unsupported cases with positive installed cgx2 build PATH --out and cgx2 emit PATH --target rust|python --out examples plus useful real negative destination/target assertions. Execute generated artifacts or verify their supported behavior with supplied pinned tools; do not auto-bless captured outputs. Update COMMANDS.md, INSTALL.md, acceptance inventory and relevant examples for package cgx2 api --json and Python .pyz output. Keep absent test/verify/notebook/bind integration and other future language/runtime capabilities honestly reserved.

Run the checker against supplied integrated binaries, all 18+ harness tests, and an outside-checkout installation smoke. Preserve current verifier/core semantics; avoid the known nested-division growth case until its separate repair lands. Use caller-supplied pins; no package installation. Keep all external rows blocked/unrun where evidence is absent and acceptance_complete false. Record original baseline as historical provenance and current suite candidate separately without rewriting past evidence. Commit updated guide, fixtures and evidence and stop.

## Stop condition

Record exact commands/results, limitations, files and commit. Update MegaMemory and stop for coordinator review. No merge, delegation, additional chats, unrelated feature work or external installation.
