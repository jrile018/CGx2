#!/usr/bin/env python3
"""Record one bounded command, including failures and signals, without replacing attempts."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(HERE.parent / "owned-result-runner-evidence"))
from supervisor import supervise, evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("label")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    target = ROOT / "target/release-managed-equality-repair"
    temporary = ROOT / "target/tmp-managed-equality-repair"
    out = HERE / "evidence"
    for p in [target, temporary, out]:
        p.mkdir(parents=True, exist_ok=True)
    prefix = out / args.label
    if prefix.with_suffix(".json").exists():
        raise RuntimeError("attempt label already exists; choose a fresh label")
    env = {**os.environ, "RUSTUP_TOOLCHAIN": "1.98.1", "CARGO_BUILD_JOBS": "2",
           "CARGO_TARGET_DIR": str(target), "TMPDIR": str(temporary),
           "LIBRARY_PATH": "/tmp/cgx2-wave22-lib", "PYTHONDONTWRITEBYTECODE": "1"}

    def limits():
        resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
        resource.setrlimit(resource.RLIMIT_CPU, (900, 900))
        resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2, 512 * 1024**2))

    result = supervise(command, cwd=ROOT, env=env, limits=limits,
                       cap=8 * 1024**2, timeout=900, merged=True)
    raw = result["stdout"]
    log = prefix.with_suffix(".log.gz")
    log.write_bytes(gzip.compress(raw, mtime=0))
    record = {"command": command, "cwd": str(ROOT), **evidence(result, include_output=False),
              "passed": result["reason"] == "exit" and result["exit"] == 0,
              "log": log.name, "log_sha256": hashlib.sha256(raw).hexdigest(),
              "environment": {k: env[k] for k in ["RUSTUP_TOOLCHAIN", "CARGO_BUILD_JOBS",
                  "CARGO_TARGET_DIR", "TMPDIR", "LIBRARY_PATH"]},
              "bounds": {"wall_seconds": 900, "cpu_seconds_per_process": 900,
                  "address_space_bytes_per_process": 4 * 1024**3,
                  "retained_combined_bytes": 8 * 1024**2, "aggregate_cgroup_isolation": False}}
    rows = re.findall(rb"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", raw)
    if rows:
        record["test_counts"] = {key: sum(int(r[i]) for r in rows)
                                  for i, key in enumerate(["passed", "failed", "ignored"])}
    record["runtime_graph_sha256"] = hashlib.sha256((ROOT / "crates/cgx2-runtime/src/graph.rs").read_bytes()).hexdigest()
    prefix.with_suffix(".json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps(record))
    if not record["passed"]:
        print(raw[-6000:].decode(errors="replace"))
    return 0 if record["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
