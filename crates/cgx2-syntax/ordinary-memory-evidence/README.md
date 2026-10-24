# Ordinary-memory implementation checkpoint

This is an **incomplete** implementation of the connected milestone in
`plans/release-completion/s2b-ordinary-memory/plan.md`. It must not advance the
ordinary-memory acceptance gate or authorize consumer fanout.

The source producer connects lexical named regions, region-generic helpers,
contextual allocation, managed graph copying, nested guard entry, and
pre-Session copied-scalar fallback to a shared interpreter/native engine.
The runtime graph image and promotion APIs preserve aliases/cycles and the
original owner on failed promotion. The v3 library adapter checks both transfer
outcomes, but is not general aggregate ownership source lowering.

The additive interface supports per-function transitive resource effects,
AST-only and source-bundle provenance, public memory API v2, and a separate
checked memory-package certificate. Source/package checking, interpretation,
native debug/release, Rust emission and artifact-preserving Python rejection
use that certificate. Source inputs and canonical targets are rechecked before
publication. Static dependency diagnostics still use the entry path; runtime
errors retain their callee source file and UTF-8 span.

Missing work includes general aggregate ownership and projected loans,
source `copy_owned`/`promote` with failed-owner continuations, complete v3
summaries/authentication and aggregate ownership package integration, exact
static dependency diagnostic attribution, and the complete required
fault/cleanup/parity matrix. Rejection of a required capability is an incomplete
row. Leak-enabled assurance, independent audit and all broader release gates
remain open. No performance measurements were made.

## Reproduce

Use the installed Rust 1.98.1 toolchain and existing offline dependencies:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-ordinary-memory"
export TMPDIR="$PWD/target/tmp-ordinary-memory"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib PYTHONDONTWRITEBYTECODE=1
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
python3 crates/cgx2-syntax/ordinary-memory-evidence/verify.py \
  --output-dir "$TMPDIR/new-verification"
python3 crates/cgx2-syntax/ordinary-memory-evidence/run.py \
  --binary-dir "$TMPDIR/new-verification/frozen" \
  --output-dir "$TMPDIR/new-product" --regressions
python3 crates/cgx2-syntax/ordinary-memory-evidence/standalone.py \
  --product-dir "$TMPDIR/new-product" \
  --output-dir "$TMPDIR/new-standalone"
python3 crates/cgx2-syntax/ordinary-memory-evidence/export.py \
  --scratch-dir "$TMPDIR" \
  --verification-dir "$TMPDIR/new-verification" \
  --product-dir "$TMPDIR/new-product" \
  --standalone-dir "$TMPDIR/new-standalone" \
  --output-dir "$TMPDIR/new-export"
```

Verification finishes all Cargo checks before its final build and artifact
freeze. Existing worker/HTTP fixtures require local child-process and loopback
access. Every output directory must be new; previous attempts remain preserved.
No Cargo writes may occur between freezing and completing parity checks.

The product runner bounds ordinary commands to 180 seconds wall time,
120 seconds CPU, 4 GiB virtual address space, 512 MiB output files, and
2 MiB captured output per stream. Regression parents receive a bounded
3,600-second wall and 1,800-second CPU envelope; their child bounds remain in
force. These are process limits, not aggregate cgroup isolation. Process groups
are cleaned up on failure or timeout. Signals, timeouts, spawn failures and
unrelated diagnostics cannot satisfy expected rejection.

Reports retain argv/cwd, exit/signal/reason, output counts/hashes, runner and
fixture hashes, compiler inputs and frozen artifact hashes. Standalone proof
copies emitted Rust to an actual `/tmp` directory and compiles/runs it without
repository libraries or working-directory assumptions. Memory parity excludes
pause observations while retaining charges, roots, failures and collection
counts. The historical owned-Result runner's single obsolete shadow-builtin
failure is checked separately from reconciled593/restored17.

## Evidence interpretation

`evidence/acceptance.json` is the acceptance ledger. Compressed raw evidence
and its manifest distinguish final checks from exploratory failures. Earlier
attempts include incorrect fixture effects, diagnostic stream handling,
preexec limits and probe-library snapshots. A later audit reproduced forbidden
`memory.default_region()` access in failed-Session fallback; direct/transitive
operation checking and UTF-8 negatives now cover that failure.

The pre-interface checkpoint passed 1,312 workspace tests (six ignored),
114 product/regression checks and 19 outside-repository standalone checks.
It remains archived in the attempt history; final results and all missing rows
are recorded in the current ledger. Root Cargo manifests remain unchanged;
no root-lock reconciliation is required. See `../S2B-ORDINARY-MEMORY.md` for
normative semantics and implementation limits.

One exploratory interface compile log was overwritten during an immediate rerun.
The ledger records that evidence gap explicitly; final verification and product
logs are retained, and the complete historical-preservation row remains open.

Final verification passes 1,327 workspace tests (six ignored), Clippy with
warnings denied, formatting, diff checking and the final offline build.
The final bounded matrix passes 117 assertions; the unchanged-artifact replay
contributes 18 regression commands plus the separate historical-negative check
(136 combined assertions). Actual outside-repository standalone proof passes
22 assertions. Product7's six incorrect-package-argv failures remain in its
parent report; only its independently validated successful regression rows are
reused by the final ledger. Product8 fixes `--package` without a compiler change.
The standalone quota failures and cleanup-audit limitation are recorded in the
ledger alongside the earlier overwritten exploratory compile log.
