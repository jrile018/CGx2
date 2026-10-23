#!/bin/sh
# Read-only checks of the Python/notebook admission boundaries.
set -eu
: "${CARGO_TARGET_DIR:?set a private absolute CARGO_TARGET_DIR}"
: "${TMPDIR:?set a private absolute TMPDIR}"
export RUSTUP_TOOLCHAIN=1.98.1
cgx2_probe_root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$cgx2_probe_root"
cargo build --offline -p cgx2-syntax -p cgx2-sema -p cgx2-emit-python -p cgx2-notebook-kernel -p cgx2-notebook-format
rustc --edition=2024 crates/cgx2c/tests/probes/conditional_consumers.rs \
    -L "dependency=$CARGO_TARGET_DIR/debug/deps" \
    --extern "cgx2_syntax=$CARGO_TARGET_DIR/debug/libcgx2_syntax.rlib" \
    --extern "cgx2_sema=$CARGO_TARGET_DIR/debug/libcgx2_sema.rlib" \
    --extern "cgx2_emit_python=$CARGO_TARGET_DIR/debug/libcgx2_emit_python.rlib" \
    --extern "cgx2_notebook_kernel=$CARGO_TARGET_DIR/debug/libcgx2_notebook_kernel.rlib" \
    --extern "cgx2_notebook_format=$CARGO_TARGET_DIR/debug/libcgx2_notebook_format.rlib" \
    -o "$CARGO_TARGET_DIR/conditional-consumers"
"$CARGO_TARGET_DIR/conditional-consumers"
