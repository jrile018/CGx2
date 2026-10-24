#!/usr/bin/env python3
"""Retain one bounded command, including failures; never overwrite an attempt."""
import json
import os
from pathlib import Path
import resource
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "crates/cgx2-syntax/owned-result-runner-evidence"))
from supervisor import supervise, evidence


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (8 * 1024**3, 8 * 1024**3))
    resource.setrlimit(resource.RLIMIT_CPU, (1200, 1200))
    resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2, 512 * 1024**2))


if __name__ == "__main__":
    dest = Path(sys.argv[1])
    dest.parent.mkdir(parents=True, exist_ok=True)
    with dest.open("x") as handle:
        record = {"argv": sys.argv[2:], "cwd": str(ROOT),
                  "environment": {k: os.environ.get(k) for k in (
                      "CARGO_TARGET_DIR", "TMPDIR", "CARGO_BUILD_JOBS",
                      "RUSTUP_TOOLCHAIN", "LIBRARY_PATH", "PYTHONDONTWRITEBYTECODE")},
                  "bounds": {"wall_seconds": 1800, "cpu_seconds": 1200,
                             "address_space_bytes": 8 * 1024**3,
                             "file_size_bytes": 512 * 1024**2,
                             "retained_bytes_per_stream": 2 * 1024**2,
                             "aggregate_isolation": False}}
        handle.write(json.dumps(record) + "\n")
        handle.flush()
        result = supervise(sys.argv[2:], cwd=str(ROOT), env=dict(os.environ),
                           limits=limits, cap=2 * 1024**2, timeout=1800)
        record.update(evidence(result))
        handle.seek(0)
        handle.write(json.dumps(record, indent=2) + "\n")
        handle.truncate()
    print(json.dumps({k: record.get(k) for k in ("argv", "exit", "reason", "duration_seconds")}))
    sys.exit(0 if record["reason"] == "exit" and record["exit"] == 0 else 1)
