#!/usr/bin/env python3
"""One immutable bounded command record, using the reviewed output supervisor."""
import json
import os
from pathlib import Path
import resource
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "owned-result-runner-evidence"))
from supervisor import evidence, supervise


def limits():
    # Regression parents legitimately lower this to their frozen 1,800-second
    # ceiling. A smaller inherited hard limit prevents their preexec from spawning.
    resource.setrlimit(resource.RLIMIT_CPU, (2400, 2400))
    resource.setrlimit(resource.RLIMIT_AS, (8 * 1024**3, 8 * 1024**3))
    resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2, 512 * 1024**2))


def main():
    label, *argv = sys.argv[1:]
    output = HERE / "checks" / (label + ".json")
    output.parent.mkdir(exist_ok=True)
    if output.exists():
        raise SystemExit("refusing to replace " + str(output))
    env = dict(os.environ, RUSTUP_TOOLCHAIN="1.98.1", CARGO_BUILD_JOBS="2",
               CARGO_INCREMENTAL="0", PYTHONDONTWRITEBYTECODE="1",
               CARGO_TARGET_DIR=str(ROOT / "target/release-ordinary-runtime-roots"),
               TMPDIR=str(ROOT / "target/tmp-ordinary-runtime-roots"),
               LIBRARY_PATH="/tmp/cgx2-wave22-lib")
    for key in ("CARGO_TARGET_DIR", "TMPDIR"):
        Path(env[key]).mkdir(parents=True, exist_ok=True)
    record = {"argv": argv, "cwd": str(ROOT),
              "environment": {k: env[k] for k in ("RUSTUP_TOOLCHAIN", "CARGO_BUILD_JOBS",
                  "CARGO_INCREMENTAL", "PYTHONDONTWRITEBYTECODE", "CARGO_TARGET_DIR",
                  "TMPDIR", "LIBRARY_PATH")},
              "limits": {"wall_seconds": 1200, "cpu_seconds_per_process": 2400,
                         "address_bytes_per_process": 8 * 1024**3,
                         "file_bytes": 512 * 1024**2, "retained_bytes_per_stream": 131072}}
    output.write_text(json.dumps(dict(record, status="running"), indent=2) + "\n")
    result = supervise(argv, cwd=ROOT, env=env, limits=limits, cap=131072, timeout=1200)
    record.update(evidence(result))
    output.write_text(json.dumps(record, indent=2) + "\n")
    for stream in ("stdout", "stderr"):
        sys.stdout.buffer.write(result[stream])
    print(json.dumps({"record": str(output), "exit": result["exit"],
                      "reason": result["reason"]}), flush=True)
    return result["exit"] if result["reason"] == "exit" and result["exit"] >= 0 else 1


if __name__ == "__main__":
    sys.exit(main())
