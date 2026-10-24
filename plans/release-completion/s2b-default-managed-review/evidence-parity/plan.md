# Managed acceptance parity and rejection evidence repair
Status: bounded evidence repair of reviewed bc4ba95. Core authentication and runtime equality repairs run independently; this lane consumes existing frozen source/runtime/test hooks and does not edit their implementation.

Read project AGENTS/design/validation/roadmap and release-completion common files, s2b-default-managed plan and review README. Use MegaMemory.

## Exclusive ownership
crates/cgx2-syntax/default-managed-evidence/run.py and verify.py; new fixtures, bounded runner helpers/tests and a new repair report/attempt directory under crates/cgx2-syntax/default-managed-evidence/**.
Existing summary.json, prior evidence attempts, source fixtures and historical hashes/logs are immutable. Existing README may gain a clearly labeled repair addendum only. Root Cargo files, all implementation sources, other evidence directories, shared docs and coordinator plans are read-only. No new AST/type/runtime/public test-hook API; report implementation blockers precisely.

## Required result
1. Extend source product evidence to native debug, native release and independent standalone for real zero/exact/one-under allocation budgets226/225, root bookkeeping64/63, nested outer258/257 with inner226, cumulative nonrefund451, and hidden scalar/package allocations. Preserve interpreted expected results, exact recovered failures, collection_count zero under actual guards and final/baseline root census.
2. Exercise labeled synthetic host/graph/root allocation failure indices2..15 across the same targets using existing failure-injection-test CLI API and emitted fault-injection feature/CGX2_TEST_FAIL_RESERVE hook. Record actual outcomes and primary error/spans; distinguish recoverable inside-expression failures from uncaught Session/guard-entry failures. Do not count injected error exits or signals as recovered success. No new production hooks.
3. Static ref-output-reject, collect-guard-reject and dishonest-package currently accept any exit1. Require expected stable diagnostic code and source span (including UTF-8 case) and test native debug/release build refusal plus Rust/Python emission refusal before overwriting existing artifacts. Preserve each sentinel's bytes. Runtime error parity must retain exact primary result, deterministic memory observations and cleanup.
4. Keep live nonblocking stdout/stderr bounds, wall/CPU/address-space/file limits and complete command, exit/signal/reason, source/runtime/binary hashes. Add meaningful runner selftests showing wrong diagnostic, unexpected exit/signal and artifact replacement fail. Retain all failed attempts. If running against the current base exposes a real implementation defect owned by another repair, report it as failed with minimal reproduction; never weaken expected behavior or broaden ownership to make the matrix green.

## Verification
Private target/release-managed-evidence-repair and target/tmp-managed-evidence-repair, CARGO_BUILD_JOBS=2, RUSTUP_TOOLCHAIN=1.98.1 and existing LIBRARY_PATH. Build necessary tools offline/locked, run the strengthened matrix and selftests, Python syntax/diff checks. Report whether each result is source-level real budget evidence or synthetic fault evidence. No performance benchmarking or leak-host reruns; existing leak negative-control block remains open. Preserve original known shadow-builtin negative separately.
Commit the bounded harness/fixture/report change and stop for review. Coordinator reruns the combined repaired code; no dependent consumer task or release gate is authorized by this repair.
