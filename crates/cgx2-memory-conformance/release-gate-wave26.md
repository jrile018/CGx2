# Wave 26 conditional source/native memory evidence

Bounded deliverable based on `a574cea65d801c284e86c057bbab3fdd7ebb4549`,
verified on 2026-10-23 in the attached `bb53/lang` worktree. The implementation
and this evidence are committed together; the handoff identifies that commit.
No root Cargo file, shared design/roadmap/ledger, notebook/Python/verifier source,
other lane, runtime API, or collector policy changed. No merge or next wave.

## Result and files

- `cgx2-sema/src/memory.rs` admits the existing mandatory-else memory statement
  in both checking paths. Exact Boolean/comparison guards, byte/token declaration
  restrictions, per-arm internal names, explicit normal-path release, recursive
  call/collect/close discovery, and conservative `Op::If` joins are checked.
  `cgx2-sema/tests/conditional_memory.rs` replaces the obsolete blanket rejection
  test with seven bounded admission/rejection tests. The existing ownership IR
  and checker are reused unchanged.
- `cgx2-cli/src/memory.rs` lowers separate selected arms and names, restores the
  outer source environment, and unroots/releases created locals on failure.
  `cgx2-emit-rust/src/memory.rs` and `memory_runtime.rs.txt` generate matching
  code and cleanup. No public scalar/record signature changed.
- Failure observers in interpreter and generated support now recognize
  `NotObject` at reference slot zero as a live byte buffer with zero edges. The
  old observer treated byte roots as unobservable; the new conditional byte
  failure tests exposed this bounded reporting defect. Original spans, first
  diagnostics, rollback observations and cleanup errors remain intact.
- `cgx2c/tests/fixtures/conditional_memory_*.cg` cover successful scoped byte
  lifetimes, all selected guard forms, nested functions/guards, preexisting
  graph/reference operations, automatic collection, region-limit/budget/signed
  size failures, and deterministic reserve faults. `failure_report_parity.rs`
  compares interpreter and generated debug/release diagnostics, spans, per-region
  semantic fields, totals, roots, edges, runtime registrations, first failure,
  operation snapshots and final unwind. Session identifiers and measured pause
  durations/buckets differ by run; region mode/trigger/limits, pause sample
  counts and all allocation/collection/reclamation counters are compared.
- `cgx2-memory-conformance/tests/conditional_source.rs` independently predicts
  `SLOT_METADATA_BYTES + payload` for 16 payload sizes and both branch outcomes,
  each at an exact two-allocation budget and one byte short: 64 source runs.
  Dropping roots does not refund budgets, trigger manual collection or commit a
  failed allocation. The automatic fixture also proves one safepoint reclaims
  1 MiB before an injected payload reserve failure, with no new committed object.
- `cgx2-sema/CONDITIONAL-MEMORY.md` documents the exact profile; owned READMEs link
  it. The `cgx2c/scripts` probes are reproducible without modifying consumers.

## Commands and results

All Cargo work used offline mode. Toolchain:
`rustc 1.98.1 (48a229cea 2026-09-01)`. Commands ran from the worktree root with
this common environment (the bridge path is the existing host compatibility
library directory):

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_TARGET_DIR="$PWD/target/wave26"
export TMPDIR="$PWD/target/wave26-tmp"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
```

The final complete owned checks, all exit 0:

```sh
cargo test --offline -p cgx2-ownership -p cgx2-sema -p cgx2-cli -p cgx2-emit-rust -p cgx2c -p cgx2-memory-conformance -p cgx2-runtime --all-features
cargo test --offline --release -p cgx2-ownership -p cgx2-sema -p cgx2-cli -p cgx2-emit-rust -p cgx2c -p cgx2-memory-conformance -p cgx2-runtime --all-features
cargo clippy --offline -p cgx2-ownership -p cgx2-sema -p cgx2-cli -p cgx2-emit-rust -p cgx2c -p cgx2-memory-conformance -p cgx2-runtime --all-targets --all-features --no-deps -- -D warnings
cargo fmt -p cgx2-ownership -p cgx2-sema -p cgx2-cli -p cgx2-emit-rust -p cgx2c -p cgx2-runtime -p cgx2-memory-conformance -- --check
rustfmt --edition=2024 --check crates/cgx2c/tests/probes/conditional_consumers.rs
git diff --check
```

Each final debug/release test run passed **340 tests across 87 test/doc-test
executions**, with zero failures or ignored tests. Existing ownership, fault
injection, runtime pressure/graph, source/native, CLI, scalar/record and compiler
output-preservation tests remain in these suites. Owned Clippy emitted no
warnings. Earlier default-feature owned tests also passed.

Read-only consumer checks (exit 0):

```sh
cargo test --offline -p cgx2-emit-python -p cgx2-notebook-kernel -p cgx2-verify-core -p cgx2-syntax
sh crates/cgx2c/scripts/verify_conditional_consumers.sh
```

The consumer suite passed **172 tests across 28 executions**, zero failures or
ignored tests. The standalone probe fresh-checks the successful script,
function and graph fixtures, then observes Python `CGE001` for memory, notebook
`CGNK001` for top-level conditional cells and `CGNK011` for effectful functions.
Kernel execution rejects them with no output saved. Conditional verification
regressions remain scalar-only. No live solver call is claimed by this run.

Dynamic instrumentation (exit 0):

```sh
python3 crates/cgx2c/scripts/verify_conditional_asan.py
```

**22 generated binary runs passed**: 11 success/failure/fault cases in both
native debug and optimized profiles. The script enables `-Zsanitizer=address`
using `RUSTC_BOOTSTRAP=1` on Rust 1.98.1 and instruments both the runtime rlib and
emitted program for `x86_64-unknown-linux-gnu`, with `ASAN_OPTIONS=detect_leaks=0`.
It checks primary failures and zero final runtime root registrations. The
standard library, system allocator internals, foreign runtimes and the compiler
process itself are outside this instrumentation boundary. Reserve injection
simulates a failure at a named fallible hook, not physical heap exhaustion.

Leak-enabled probe (exit 1, **blocked**, not passed):

```sh
ASAN_OPTIONS=detect_leaks=1 target/wave26/conditional-asan/debug/conditional_memory_success-0
```

It reports `LeakSanitizer has encountered a fatal error` and `LeakSanitizer does
not work under ptrace (strace, gdb, etc)`. Leak freedom remains unverified on
this host. Live solver, LLM, delegated cgroup and broad memory-safety release
gates are not claimed. This finite corpus is implementation evidence, not a
whole-language ownership, FFI, leak-freedom or allocator-independent proof.

Raw local logs and instrumented programs remain in the private ignored target
folder. The committed `evidence/wave26-verification.json` records command
results, suite counts and log hashes. Zero final registrations proves the
observed roots were released; unrooted manual objects can retain their byte
charges until collection, region close or session disposal.
