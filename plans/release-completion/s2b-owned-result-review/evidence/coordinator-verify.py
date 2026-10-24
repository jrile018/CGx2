#!/usr/bin/env python3
"""Run the required local ordinary-owned checks, preserving exact bounded logs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import signal
import subprocess
import sys
import time

ROOT = Path('/home/john-riley/.codex/worktrees/wave21-coordinator-review/lang')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True)
    options = parser.parse_args()
    output = options.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    target, temporary = Path("/home/john-riley/projects/lang/target/release-plan-review"), Path("/home/john-riley/projects/lang/target/cgx2-wave21-tmp")
    target.mkdir(parents=True, exist_ok=True)
    temporary.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, "RUSTUP_TOOLCHAIN": "1.98.1", "CARGO_BUILD_JOBS": "2", "CARGO_TARGET_DIR": str(target), "TMPDIR": str(temporary), "LIBRARY_PATH": "/tmp/cgx2-wave22-lib"}
    env["RUSTFLAGS"] = (env.get("RUSTFLAGS", "") + " -Clink-arg=-Wl,--threads=2").strip()
    binaries = target / "debug"
    commands = [
        ("build", ["cargo", "build", "--workspace", "--all-features", "--offline", "--locked"]),
        ("owned-result-product", [sys.executable, "crates/cgx2-syntax/owned-result-evidence/run.py", "--binary-dir", binaries, "--output", output / "owned-result-product.json"]),
        ("ordinary-product", [sys.executable, "crates/cgx2-syntax/ordinary-owned-evidence/run.py", "--binary-dir", binaries, "--output", output / "ordinary-product.json"]),
        ("workspace-tests", ["cargo", "test", "--workspace", "--all-features", "--offline", "--locked", "--no-fail-fast"]),
        ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--offline", "--locked", "--", "-D", "warnings"]),
        ("format", ["cargo", "fmt", "--all", "--check"]),
        ("diff", ["git", "diff", "--check"]),
        ("foundation", [sys.executable, "crates/cgx2-syntax/s2b-evidence/run.py", "--binary-dir", binaries, "--output", output / "foundation.json"]),
        ("semantic", [sys.executable, "crates/cgx2-sema/tests/semantic_resolution/run.py", "--binary", binaries / "cgx2", "--output", output / "semantic.json"]),
        ("specialization", [sys.executable, "crates/cgx2-sema/tests/source_specialization/run.py", "--binary", binaries / "cgx2", "--output-dir", output / "specialization"]),
        ("visibility", [sys.executable, "crates/cgx2-api-index/tests/verify_visibility_cli.py", "--binary", binaries / "cgx2", "--output-dir", output / "visibility"]),
        ("binders", [sys.executable, "crates/cgx2-package/scripts/verify_binders.py", "--binary-dir", binaries, "--output", output / "binders.json"]),
        ("output", [sys.executable, "crates/cgx2-cli/scripts/verify_output_bounds.py", "--binary", binaries / "cgx2", "--output", output / "output.json", "--build-address-space-mib", "4096"]),
        ("emitters", [sys.executable, "crates/cgx2-emit-python/tests/s2b_product.py", "--binary-dir", binaries, "--output", output / "emitters.json"]),
        ("legacy-graph99", [sys.executable, "crates/cgx2-syntax/s2b-memory-evidence/run.py", "--binary-dir", binaries, "--output", output / "legacy-graph99.json"]),
        ("native-guards518", [sys.executable, "crates/cgx2-emit-rust/tests/graph_guard_parity/run.py", "--binary-dir", binaries, "--output", output / "native-guards518.json"]),
        ("report-layouts265", [sys.executable, "crates/cgx2-cli/tests/memory_report_layout/run.py", "--binary", binaries / "cgx2", "--output", output / "report-layouts265.json"]),
    ]
    checks = []
    report = {"schema_version": 1, "checkpoint_complete": False, "release_complete": False,
              "environment": {k: env[k] for k in ["RUSTUP_TOOLCHAIN", "CARGO_BUILD_JOBS", "CARGO_TARGET_DIR", "TMPDIR", "LIBRARY_PATH"]}, "checks": checks}
    def limits():
        resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
        resource.setrlimit(resource.RLIMIT_CPU, (900, 900))
    for name, command in commands:
        command = [str(c) for c in command]
        path = output / (name + ".log")
        print("start " + name, flush=True)
        started = time.monotonic()
        with path.open("wb") as log:
            child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True, preexec_fn=limits)
            while child.poll() is None:
                if time.monotonic() - started > 900 or path.stat().st_size > 8 * 1024**2:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
                    break
                try:
                    child.wait(timeout=0.5)
                except subprocess.TimeoutExpired:
                    pass
            rc = child.returncode
        if path.stat().st_size > 8 * 1024 * 1024:
            raise RuntimeError(f"{name} exceeded the 8 MiB log bound")
        data = path.read_bytes()
        record = {"name": name, "command": command, "exit": rc, "log": path.name,
                  "log_sha256": hashlib.sha256(data).hexdigest(), "duration_seconds": round(time.monotonic() - started, 3)}
        if name == "workspace-tests":
            summaries = re.findall(rb"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored", data)
            record["suites"] = len(summaries)
            record.update({k: sum(int(s[i]) for s in summaries) for k, i in [("passed", 1), ("failed", 2), ("ignored", 3)]})
        checks.append(record)
        (output / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"finish {name}: exit {rc}", flush=True)
    return int(any(c["exit"] != 0 for c in checks))


if __name__ == "__main__":
    raise SystemExit(main())
