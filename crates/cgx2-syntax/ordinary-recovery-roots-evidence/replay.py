#!/usr/bin/env python3
"""Run a read-only existing harness using this lane's private environment.

The original runner and fixtures stay unchanged. Only common.Journal's private
target/TMPDIR and reviewed-base fields differ; command expectations do not.
"""
import runpy
import os
import sys
from pathlib import Path
import common

lane, runner, *arguments = sys.argv[1:]
original = common.HERE.parent / lane
common.HERE = original
sys.path.insert(0, str(original))
if lane == 'ordinary-api-repair-evidence':
    import command
    # Keep the read-only API harness and its expectations, but give every
    # subprocess this lane's already-created private target and TMPDIR.
    command.environment = lambda: dict(os.environ,
        CARGO_TARGET_DIR=str(common.ROOT / 'target/release-ordinary-recovery-roots'),
        TMPDIR=str(common.ROOT / 'target/tmp-ordinary-recovery-roots'),
        RUSTUP_TOOLCHAIN='1.98.1', CARGO_BUILD_JOBS='2', PYTHONDONTWRITEBYTECODE='1',
        LIBRARY_PATH='/tmp/cgx2-wave22-lib')
sys.argv = [str(original / runner), *arguments]
runpy.run_path(sys.argv[0], run_name="__main__")
