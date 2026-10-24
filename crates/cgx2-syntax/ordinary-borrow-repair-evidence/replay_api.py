#!/usr/bin/env python3
"""Run the unchanged API matrix with this chat's private scratch environment."""
import os
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "ordinary-api-repair-evidence"))
import command


def environment():
    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / "target/release-ordinary-borrow-nominal"),
               TMPDIR=str(ROOT / "target/tmp-ordinary-borrow-nominal"),
               CARGO_BUILD_JOBS="2", RUSTUP_TOOLCHAIN="1.98.1", PYTHONDONTWRITEBYTECODE="1",
               LIBRARY_PATH="/tmp/cgx2-wave22-lib")
    return env


command.environment = environment
runpy.run_path(str(HERE.parent / "ordinary-api-repair-evidence/run.py"), run_name="__main__")
