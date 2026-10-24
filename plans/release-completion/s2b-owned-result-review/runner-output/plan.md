# Enforce live output limits in owned Result evidence runner

Linked requirement: s2b-owned-result/plan.md mandatory bounded CPU/memory/output/process-tree checks. Existing crates/cgx2-syntax/owned-result-evidence/run.py redirects output into temporary files and waits120s before checking CAP=1MiB, so temporary output can grow without that claimed cap. Final584 product pass does not establish live output confinement.

## Read and ownership
Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, release-completion README/contracts/worker-rules, s2b-owned-result/plan.md and S2B-OWNED-RESULT.md. MegaMemory overview/query/record and Serena instructions required.
Exclusive edits: crates/cgx2-syntax/owned-result-evidence/run.py and verify.py; new crates/cgx2-syntax/owned-result-runner-evidence/**. All fixtures, expected/positive JSON, historical evidence, README/SHA256SUMS, contracts, Rust source, root/shared docs/plans are read-only. Compatibility repair owns sema/admission/contracts and its separate new fixtures. No shared interface changes.

## Deliverable and checks
Enforce bounded output while the subprocess runs, including stderr, and promptly kill/reap its process group on overflow or timeout. Keep final retained bytes bounded and avoid unbounded temporary disk capture. A pipe-draining supervisor or equivalent effective cap is acceptable; distinguish overflow/timeout/signal from expected program errors. Preserve command evidence on failure, safe UTF-8 handling, and actual diagnostics. Existing 120s wall/60s CPU/2GiB product limits and outer900s/4GiB/8MiB contract must stay explicit. Audit outer verify.py for the same output/process-group issue and make only needed corrections; periodic file checking with unbounded overshoot is not a strict file cap. No aggregate cgroup isolation claims.
Add executable harness self-tests with safe bounded synthetic children: success, nonzero error, stdout/stderr overflow, simultaneous output, timeout, descendant cleanup and non-UTF8 output. Ensure tests themselves cannot flood host disk/processes and do not modify host settings. Preserve exact exit/reason evidence for harness failures.
Run new self-tests plus unchanged full584 product suite against privately built reviewed binaries. Run syntax tests,fmt/diff; this Python-only change does not require repeating full workspace/Clippy if no Rust changes, but report exact checks and historical result distinction. Record hashes and commands under owned-result-runner-evidence, without overwriting historical product artifacts.

## Finish
Use private target/temp dirs, CARGO_BUILD_JOBS=2, RUSTUP_TOOLCHAIN=1.98.1 and LIBRARY_PATH=/tmp/cgx2-wave22-lib. Offline cached dependencies only; no manifests/lock modifications, installations, external spend/security changes, delegation/other chats, merges/releases or follow-on work. Commit bounded repair and stop for review. Full memory and release acceptance remain open.
