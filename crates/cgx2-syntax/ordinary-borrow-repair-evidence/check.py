#!/usr/bin/env python3
"""Retain bounded development commands, including failures, before freezing."""
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
    resource.setrlimit(resource.RLIMIT_FSIZE, (1024**3, 1024**3))


def main():
    label, *argv = sys.argv[1:]
    target = ROOT / "target/release-ordinary-borrow-nominal"
    temporary = ROOT / "target/tmp-ordinary-borrow-nominal"
    target.mkdir(parents=True, exist_ok=True)
    temporary.mkdir(parents=True, exist_ok=True)
    directory = Path(__file__).resolve().parent / "checks"
    directory.mkdir(exist_ok=True)
    path = directory / (label + ".json")
    if path.exists():
        raise FileExistsError(path)
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), TMPDIR=str(temporary),
               CARGO_BUILD_JOBS="2", RUSTUP_TOOLCHAIN="1.98.1",
               PYTHONDONTWRITEBYTECODE="1", LIBRARY_PATH="/tmp/cgx2-wave22-lib")
    record = dict(argv=argv, cwd=str(ROOT), bounds=dict(wall_seconds=1800,
                  cpu_seconds_per_process=1200, address_space_bytes_per_process=8 * 1024**3,
                  file_bytes=1024**3, output_bytes_per_stream=8 * 1024**2),
                  environment={k: env[k] for k in ["CARGO_TARGET_DIR", "TMPDIR", "CARGO_BUILD_JOBS",
                  "RUSTUP_TOOLCHAIN", "PYTHONDONTWRITEBYTECODE", "LIBRARY_PATH"]}, status="started")
    path.write_text(json.dumps(record, indent=2) + "\n")
    result = supervise(argv, cwd=str(ROOT), env=env, limits=limits, cap=8 * 1024**2, timeout=1800)
    record.update(evidence(result), status="complete")
    path.write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({k: record.get(k) for k in ["argv", "reason", "exit", "signal"]}), flush=True)
    print(record["stdout"][-7000:], flush=True)
    print(record["stderr"][-3500:], flush=True)
    return 0 if result["reason"] == "exit" and result["exit"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
