# Managed core repair evidence

Read [the contract and results](../S2B-DEFAULT-MANAGED-AUTH-REPAIR.md). Original evidence and coordinator plans are read-only. `attempt-*` preserve this runner's successive successful attempts; snapshots identify older runner versions. `verification-final/` contains the final inventory and compressed original command output/product reports, including failures. `verification/` is a superseded pre-freeze collection with an explicit notice.

Reproduce in this attached worktree using existing dependencies:

```sh
mkdir -p target/release-managed-core-repair target/tmp-managed-core-repair
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-managed-core-repair"
export TMPDIR="$PWD/target/tmp-managed-core-repair"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
cargo build --offline --locked --all-features -p cgx2-sema -p cgx2-package -p cgx2-cli -p cgx2-api-index -p cgx2-emit-rust -p cgx2c
cargo test --offline --locked --all-features --no-fail-fast -p cgx2-sema -p cgx2-package -p cgx2-cli -p cgx2-api-index -p cgx2-emit-rust -p cgx2c
cargo clippy --offline --locked --all-features --all-targets -p cgx2-sema -p cgx2-package -p cgx2-cli -p cgx2-api-index -p cgx2-emit-rust -p cgx2c -- -D warnings
cargo fmt --all --check
git diff --check
cargo build --offline --locked --all-features -p cgx2-sema -p cgx2-package -p cgx2-cli -p cgx2-api-index -p cgx2-emit-rust -p cgx2c -p cgx2-effect-index -p cgx2-security -p cgx2-verify-cli -p cgx2-notebook-cli
python3 crates/cgx2-syntax/managed-core-repair-evidence/freeze.py --binary-dir "$CARGO_TARGET_DIR/new-verified-bin" --output-dir "$CARGO_TARGET_DIR/new-verified-bin" --manifest "$TMPDIR/new-frozen-binaries.json"
python3 crates/cgx2-syntax/managed-core-repair-evidence/run.py --binary-dir "$CARGO_TARGET_DIR/new-verified-bin" --output-dir "$TMPDIR/new-core-attempt"
python3 crates/cgx2-syntax/owned-symbol-compatibility-evidence/run.py --binary-dir "$CARGO_TARGET_DIR/new-verified-bin" --output "$TMPDIR/new-symbols/product.json"
python3 plans/release-completion/s2b-owned-result-review/reconcile.py --binary-dir "$CARGO_TARGET_DIR/new-verified-bin" --output-dir "$TMPDIR/new-reconciliation"
python3 crates/cgx2-syntax/default-managed-evidence/run.py --binary-dir "$CARGO_TARGET_DIR/new-verified-bin" --output-dir "$TMPDIR/new-shapes" --only shapes
python3 crates/cgx2-syntax/default-managed-evidence/run.py --binary-dir "$CARGO_TARGET_DIR/new-verified-bin" --output-dir "$TMPDIR/new-package" --only package
python3 crates/cgx2-syntax/default-managed-evidence/run.py --binary-dir "$CARGO_TARGET_DIR/new-verified-bin" --output-dir "$TMPDIR/new-namespace" --only namespace-package
python3 crates/cgx2-syntax/default-managed-evidence/run.py --binary-dir "$CARGO_TARGET_DIR/new-verified-bin" --output-dir "$TMPDIR/new-hybrid" --only hybrid
```

The unfiltered test command intentionally still exposes the coordinator-owned obsolete layout-test constructor call, exit 101. The new product runner's temporary migrated copy retains all original assertions. Do not hide that failure or edit its historical test from this lane. Source snapshots are immutable authentication inputs; checked package and compiler publication additionally validate live filesystem provenance.
