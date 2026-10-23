#!/bin/sh
# Read-only checks of the Python/notebook admission boundaries.
set -eu
: "${CARGO_TARGET_DIR:?set a private absolute CARGO_TARGET_DIR}"
: "${TMPDIR:?set a private absolute TMPDIR}"
export RUSTUP_TOOLCHAIN=1.98.1
cgx2_probe_root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$cgx2_probe_root"
cargo build --offline -p cgx2-syntax -p cgx2-sema -p cgx2-emit-python -p cgx2-notebook-kernel -p cgx2-notebook-format
rustc --edition=2024 crates/cgx2c/tests/probes/range_consumers.rs \
    -L "dependency=$CARGO_TARGET_DIR/debug/deps" \
    --extern "cgx2_syntax=$CARGO_TARGET_DIR/debug/libcgx2_syntax.rlib" \
    --extern "cgx2_sema=$CARGO_TARGET_DIR/debug/libcgx2_sema.rlib" \
    --extern "cgx2_emit_python=$CARGO_TARGET_DIR/debug/libcgx2_emit_python.rlib" \
    --extern "cgx2_notebook_kernel=$CARGO_TARGET_DIR/debug/libcgx2_notebook_kernel.rlib" \
    --extern "cgx2_notebook_format=$CARGO_TARGET_DIR/debug/libcgx2_notebook_format.rlib" \
    -o "$CARGO_TARGET_DIR/range-consumers"
"$CARGO_TARGET_DIR/range-consumers"
cargo build --offline -p cgx2-verify-cli
"$CARGO_TARGET_DIR/debug/cgx2-verify" crates/cgx2c/tests/probes/range_verifier.cg --function memory_loop --format json > "$CARGO_TARGET_DIR/range-verifier.json"
python3 - "$CARGO_TARGET_DIR/range-verifier.json" <<'PY'
import json
import sys
from pathlib import Path
report = json.loads(Path(sys.argv[1]).read_text())
assert report["proof"]["kind"] == "unknown", report
assert report["proof"]["state"] == "unsupported", report
assert report["verdict"]["kind"] == "unknown", report
assert report["verdict"]["diagnostic"]["code"] == "CGV001", report
print("scalar verifier: unknown/unsupported (CGV001); no solver invoked")
PY
