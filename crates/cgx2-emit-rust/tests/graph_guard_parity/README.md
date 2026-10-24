# Native graph guard repair

This slice changes only the legacy Rust memory generator and this test tree.
`MemoryEmitContext::graph_profile` is computed once from the freshly checked
module with `cgx2_sema::graph_memory_profile`. Every emitted function, branch,
loop and lexical guard receives that immutable context. Graph modules enter
with `Session::try_no_gc(budget)`; other modules retain `Session::no_gc(budget)`.
An entry failure becomes CGR013 at the original guard statement span. A result
closure ends the guard borrow before recording failure, while failures inside
the body retain their first observation before branch/function root cleanup.
No runtime, AST, type, CLI or Python API changes or dependencies are introduced.

## Executable coverage

The 17 `.cg` cases execute through the interpreter, native debug, native release
and standalone emitted Rust. Standalone builds copy and hash the existing
runtime source files and compile those copies outside repository-relative
execution assumptions. Reports compare diagnostic code, independently computed
source spans, operation/final-unwind roots, cleanup errors, rollback observations
and all deterministic memory counters (session identity and pause timing are
excluded). Runtime failures must exit normally with code 1; signals, crashes,
timeouts and unrelated errors fail the runner.

- `nested-zero.cg` preserves the reviewed CGR013 BudgetExceeded at 179..251.
  `outer-zero.cg` also exercises a top-level script.
- `inner-zero` succeeds with outer budget 32 and inner budget zero: entry charges
  the existing guard, not the new guard. `outer-one-under` fails at 31.
- The three-level exact budgets are outer 80 / middle 48 / inner 0; separate
  79 / 48 / 0 and 80 / 47 / 0 cases fail at the third entry.
- `call-*` sends those charges across a scalar callee. The outer guard also
  pays `OWNED_METADATA_BYTES=32` for the callee's zero-byte owned value, so the
  exact total is 112. Separate outer 111 and middle 47 cases fail at entry.
- `callee-zero` executes a scalar-only callee before graph construction;
  `caller-zero` guards a caller whose graph operations are only in its callee.
  `branch-loop-zero` reaches guards through nested branch and loop lowering.
  `body-failure` checks failure observation before branch/callee cleanup.
- Legacy byte allocations retain the 96-byte exact / 95-byte failure boundary,
  and legacy nested zero-budget entry still succeeds when its body allocates
  nothing. These concrete guard-table fixtures use the tested 64-bit runtime's
  16-byte `Budget` entries and exact-capacity table growth.

`graph_guard_parity.rs` additionally executes generated debug and optimized
Rust with the supported runtime `fault-injection` feature and
`Session::inject_host_failure_at_reserve`. The synthetic reserve sweep identifies
all three `ReserveSiteV2::GuardBudgets` failures, checks exact parsed guard spans,
preserves CGR013 HostAllocation, releases all roots without collection, and
allows retry. A direct runtime check proves failed nested entry keeps the outer
guard active and does not charge its budget. These are **synthetic** host reserve
failures, not observations of real host exhaustion or leak assurance. The new
regression fails normally against the original generator (zero guard reserve
failures instead of three); `evidence/regression-before.log` records that run.

## Reproduction

From this worktree:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-native-guards"
export TMPDIR="$PWD/target/tmp-native-guards"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test -p cgx2-emit-rust --all-features --offline --locked
cargo test -p cgx2-emit-rust --all-features --offline --locked --test graph_guard_parity -- --nocapture
cargo build -p cgx2-cli -p cgx2c --offline --locked
python3 crates/cgx2-emit-rust/tests/graph_guard_parity/run.py --binary-dir "$CARGO_TARGET_DIR/debug" --output "$TMPDIR/guard-product.json"
python3 crates/cgx2-syntax/s2b-memory-evidence/run.py --binary-dir "$CARGO_TARGET_DIR/debug" --output "$TMPDIR/graph-product.json"
cargo test -p cgx2-cli --all-features --offline --locked
cargo test -p cgx2c --all-features --offline --locked --test native_memory_functions --test native_memory_reports --test memory_release_parity --test failure_report_parity --test source_graph_commands
cargo test -p cgx2-runtime --all-features --offline --locked --test fallible_lifetimes
cargo clippy -p cgx2-emit-rust --all-targets --all-features --offline --locked -- -D warnings
cargo fmt --all --check
git diff --check
git diff --cached --check
```

`evidence/verification.json` records actual commands, counts, environment and log
hashes. `evidence/product.json` retains reports from the new fixtures;
`evidence/graph-product.json` retains the existing 99-check graph regression run.
Each `.log` is a JSON envelope: concatenate `combined_output_lines` to recover
the exact captured output, including whitespace. `original_sha256` authenticates
that capture. This keeps raw Cargo whitespace from failing staged diff checks.

## Unmet requirements and boundaries

The plan requests Python rejection **CGE006**. The existing legacy Python
emitter (`crates/cgx2-emit-python/src/emit.rs`, outside this lane's ownership)
returns **CGE001** for these checked memory constructs. All 17 Python attempts
are verified to exit normally with code 1 and preserve the exact prior artifact;
the requested diagnostic-code row remains **incomplete**, explicitly recorded
in product evidence. Resolving it requires coordinator-approved Python emitter
ownership and a reviewed diagnostic compatibility change. It is not repaired
by changing native guard generation or by substituting a different source
admission error.

This is a native guard repair within the explicit memory-script profile. Full
ordinary-function memory, the separate report-layout repair, remaining S2b/S3/S4
work and all R01-R15 gates remain required. This run adds no leak-enabled,
independent audit, formal proof, real resource exhaustion, performance or release
acceptance evidence. No dependencies or root Cargo reconciliation are needed.
