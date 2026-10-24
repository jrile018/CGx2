#!/usr/bin/env python3
"""Re-run the three frozen product runners without changing historical evidence."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    options = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    binaries = options.binary_dir.resolve()
    output = options.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if not os.environ.get("TMPDIR"):
        parser.error("set TMPDIR to the private worktree temporary directory")
    runners = [
        ("semantic-resolution", "cgx2-sema/tests/semantic_resolution/run.py",
         ["--binary", str(binaries / "cgx2")]),
        ("foundation", "cgx2-syntax/s2b-evidence/run.py",
         ["--binary-dir", str(binaries)]),
        ("emitter", "cgx2-emit-python/tests/s2b_product.py",
         ["--binary-dir", str(binaries)]),
    ]
    statuses = []
    for name, script, arguments in runners:
        command = [sys.executable, str(root / "crates" / script), *arguments,
                   "--output", str(output / (name + ".json"))]
        with (output / (name + ".log")).open("w") as log:
            result = subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT)
        statuses.append(dict(name=name, command=command, exit=result.returncode))
        print(f"{name}: exit {result.returncode}", flush=True)
    (output / "runner.json").write_text(json.dumps(statuses, indent=2) + "\n")
    return int(any(item["exit"] != 0 for item in statuses))


if __name__ == "__main__":
    raise SystemExit(main())
