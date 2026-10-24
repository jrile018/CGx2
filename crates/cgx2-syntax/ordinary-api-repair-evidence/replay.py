#!/usr/bin/env python3
"""Replay unchanged historical scripts using the assigned private TMPDIR."""
import os
from pathlib import Path
import runpy
import sys
import tempfile

original = tempfile.TemporaryDirectory
standalone_root = None


def private_directory(*args, **kwargs):
    if kwargs.get("dir") == "/tmp":
        kwargs["dir"] = standalone_root or os.environ["TMPDIR"]
    return original(*args, **kwargs)


if __name__ == "__main__":
    if sys.argv[1:2] == ["--standalone-root"]:
        standalone_root = str(Path(sys.argv[2]).resolve())
        Path(standalone_root).mkdir(parents=True, exist_ok=False)
        print("Historical standalone cwd parent: " + standalone_root, flush=True)
        del sys.argv[1:3]
    tempfile.TemporaryDirectory = private_directory
    script, *args = sys.argv[1:]
    sys.argv = [script, *args]
    runpy.run_path(script, run_name="__main__")
