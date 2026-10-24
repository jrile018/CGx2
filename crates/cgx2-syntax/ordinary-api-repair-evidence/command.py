#!/usr/bin/env python3
"""Retain bounded command attempts; never replace earlier evidence."""
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import resource
import sys

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "supervisor", ROOT / "crates/cgx2-syntax/owned-result-runner-evidence/supervisor.py")
supervisor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(supervisor)


def limits():
    for kind, value in [(resource.RLIMIT_CPU, 1200),
                        (resource.RLIMIT_AS, 16 * 1024**3),
                        (resource.RLIMIT_FSIZE, 256 * 1024**2),
                        (resource.RLIMIT_NOFILE, 256)]:
        resource.setrlimit(kind, (value, value))


def environment():
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(ROOT / "target/release-ordinary-api-visibility"),
               TMPDIR=str(ROOT / "target/tmp-ordinary-api-visibility"),
               CARGO_BUILD_JOBS="2", RUSTUP_TOOLCHAIN="1.98.1",
               PYTHONDONTWRITEBYTECODE="1", LIBRARY_PATH="/tmp/cgx2-wave22-lib")
    Path(env["TMPDIR"]).mkdir(parents=True, exist_ok=True)
    Path(env["CARGO_TARGET_DIR"]).mkdir(parents=True, exist_ok=True)
    return env


def main():
    label, *command = sys.argv[1:]
    path = HERE / "evidence" / (label + ".json.gz")
    path.parent.mkdir(exist_ok=True)
    if path.exists():
        raise FileExistsError(path)
    env = environment()
    result = supervisor.supervise(command, cwd=ROOT, env=env, limits=limits,
                                   cap=2 * 1024**2, timeout=1500)
    record = supervisor.evidence(result)
    record.update(argv=command, cwd=str(ROOT), environment={k: env[k] for k in
                  ["CARGO_TARGET_DIR", "TMPDIR", "CARGO_BUILD_JOBS", "RUSTUP_TOOLCHAIN",
                   "PYTHONDONTWRITEBYTECODE", "LIBRARY_PATH"]},
                  limits={"wall_seconds": 1500, "cpu_seconds_per_process": 1200,
                          "address_bytes_per_process": 16 * 1024**3,
                          "file_bytes": 256 * 1024**2, "pipe_bytes_per_stream": 2 * 1024**2},
                  supervisor_sha256=hashlib.sha256(Path(spec.origin).read_bytes()).hexdigest())
    with gzip.open(path, "wt") as stream:
        json.dump(record, stream, indent=2)
    print(json.dumps({k: record.get(k) for k in
                      ["exit", "reason", "signal", "duration_seconds", "stdout_bytes", "stderr_bytes"]}))
    for name in ["stdout", "stderr"]:
        print(record[name][-4000:])
    return 0 if result["exit"] == 0 and result["reason"] == "exit" else 1


if __name__ == "__main__":
    sys.exit(main())
