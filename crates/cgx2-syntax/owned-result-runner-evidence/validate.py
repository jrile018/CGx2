#!/usr/bin/env python3
"""Reproduce only the bounded runner-repair checks; keep historical evidence intact."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import sys

from supervisor import evidence, supervise

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
    resource.setrlimit(resource.RLIMIT_CPU, (900, 900))


def main():
    target, temporary = ROOT / "target/owned-result-runner", ROOT / "target/tmp-owned-result-runner"
    target.mkdir(parents=True, exist_ok=True)
    temporary.mkdir(parents=True, exist_ok=True)
    output = HERE / "results"
    output.mkdir(exist_ok=True)
    env = {**os.environ, "RUSTUP_TOOLCHAIN": "1.98.1", "CARGO_BUILD_JOBS": "2",
           "CARGO_TARGET_DIR": str(target), "TMPDIR": str(temporary),
           "LIBRARY_PATH": "/tmp/cgx2-wave22-lib", "PYTHONDONTWRITEBYTECODE": "1"}
    env["RUSTFLAGS"] = (env.get("RUSTFLAGS", "") + " -Clink-arg=-Wl,--threads=2").strip()
    commands = [
        ("selftests", [sys.executable, HERE / "selftest.py"]),
        ("build", ["cargo", "build", "--workspace", "--all-features", "--offline", "--locked"]),
        ("syntax-tests", ["cargo", "test", "-p", "cgx2-syntax", "--all-features", "--offline", "--locked"]),
        ("owned-result-product", [sys.executable, HERE.parent / "owned-result-evidence/run.py",
                                  "--binary-dir", target / "debug", "--output", output / "product.json"]),
        ("fmt", ["cargo", "fmt", "--all", "--check"]),
        ("diff", ["git", "diff", "--check"]),
    ]
    report = {"base": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "checkpoint_complete": False, "release_complete": False,
              "environment": {k: env[k] for k in ("RUSTUP_TOOLCHAIN", "CARGO_BUILD_JOBS", "CARGO_TARGET_DIR", "TMPDIR", "LIBRARY_PATH", "RUSTFLAGS")},
              "checks": []}
    for name, command in commands:
        command = [str(c) for c in command]
        print("start " + name, flush=True)
        result = supervise(command, cwd=ROOT, env=env, limits=limits, cap=8 * 1024**2, timeout=900, merged=True)
        (output / (name + ".log.gz")).write_bytes(gzip.compress(result["stdout"], mtime=0))
        report["checks"].append({"name": name, "command": command, "log": name + ".log.gz", "log_encoding": "gzip",
                                  **evidence(result, include_output=False)})
        (output / "checks.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"finish {name}: {result['reason']} {result['exit']}", flush=True)
        if result["reason"] != "exit" or result["exit"] != 0:
            return 1
    files = [HERE.parent / "owned-result-evidence" / n for n in ("run.py", "verify.py", "positive.json", "expected.json")]
    files += sorted((HERE.parent / "owned-result-evidence/fixtures").glob("*.cg"))
    files += sorted((target / "debug").glob("cgx2*"))
    files += [HERE / "README.md"] + sorted(HERE.glob("*.py"))
    files += sorted(p for p in output.glob("*") if p.name != "hashes.json")
    hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file()}
    (output / "hashes.json").write_text(json.dumps(hashes, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
