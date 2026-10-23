# CGx2 bounded SMT emission repair

## Goal

Repair the reviewed P1 exponential SMT emission growth before Stage 2 language work.

Read the parent review README.md, master completion plan, frozen contracts, common worker rules, design/validation plan and local AGENTS.md. This is an authorized review repair, not Stage 2.

## Exclusive write ownership

- `crates/cgx2-verify-core/**`
- `crates/cgx2-verify-cli/**`

Root Cargo files, all shared plans and every other lane are read-only. Preserve frozen interfaces consumed by the other two repair chats. Use private target/TMPDIR and CARGO_BUILD_JOBS=2. If /tmp quota still prevents sandbox startup, report it accurately and use the normal approval mechanism for scoped commands with disk-backed TMPDIR; do not delete unrelated temporary data.

## Required work and acceptance

The integrated scalar_v2 encoder interpolates child terms repeatedly at src/scalar_v2/smtlib.rs:193–219. A chain of divisions grows roughly sevenfold per level before solver deadlines apply. Coordinator reproduced actual source input with only 1/2/3/4 divisions: 104/108/112/116 source bytes generated 3801/23325/158517/1103385 SMT bytes. Those runs were capped at 512 MiB and 15 seconds; do not reproduce larger unbounded cases.

Implement bounded, compact emission with lexical bindings or equivalent shared-expression representation. Preserve lexical scoping, eager checked arithmetic, branch-definedness, division truncation toward zero, zero/MIN divided by -1 failures, uninterpreted-function assumptions and source/core/SMT audit independence. Bound output allocation before expanding oversized text; a post-hoc length check after huge allocation is insufficient. Define and document node/depth/output-byte budgets and how limits propagate as unsupported/unknown or a structured failure before solver launch. Cover main obligation, witness, probe and audit paths; no panic from expect on a newly fallible bounded path. Preserve legacy root APIs and public source semantics; do not change shared parser/checker.

Add small measured-growth regressions, near/over-budget tests, lexical-collision/shadowing tests, inactive-branch and integer-edge parity, and corruption rejection. Confirm roughly linear/bounded growth for nested division with safe in-process limits; retain actual SMT parity instead of merely skipping audit. Run both owned suites, Clippy/fmt and CLI bounded reproduction. Genuine solver acceptance stays blocked unless a previously trusted installed solver is supplied. Write an owned report, commit and stop.

## Stop condition

Record exact commands/results, limitations, files and commit. Update MegaMemory and stop for coordinator review. No merge, delegation, additional chats, unrelated feature work or external installation.
