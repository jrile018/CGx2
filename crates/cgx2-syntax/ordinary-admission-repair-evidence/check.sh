#!/bin/sh
set -eu
export CARGO_TARGET_DIR="$PWD/target/release-ordinary-core-admission"
export TMPDIR="$PWD/target/tmp-ordinary-core-admission"
export CARGO_BUILD_JOBS=2 RUSTUP_TOOLCHAIN=1.98.1 PYTHONDONTWRITEBYTECODE=1
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
label="$1"
shift
exec python3 crates/cgx2-syntax/ordinary-admission-repair-evidence/command.py \
    "crates/cgx2-syntax/ordinary-admission-repair-evidence/$label.json" "$@"
