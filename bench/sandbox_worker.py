"""Run inside the verified Bubblewrap profile; never import a private oracle."""

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile

import isolation


def main():
    config = json.loads(Path("/workspace/.cgx2-worker.json").read_text(encoding="utf-8"))
    command = config["build_argv"]
    if not isinstance(command, list) or not all(isinstance(x, str) for x in command):
        raise ValueError("invalid worker command")
    if command[0] != config["compiler"] or not command[0].startswith("/toolchain/"):
        raise ValueError("compiler must be in read-only toolchain")
    with tempfile.TemporaryFile(dir="/scratch") as stdout, tempfile.TemporaryFile(dir="/scratch") as stderr:
        child = subprocess.Popen(command, cwd="/workspace", stdin=subprocess.DEVNULL,
                                 stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            exit_code = child.wait(timeout=90)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
            print("build_timeout", file=sys.stderr)
            return 100
        stdout.seek(0)
        stderr.seek(0)
        build_out = stdout.read(isolation.LIMITS["output_bytes"] + 1)
        build_err = stderr.read(isolation.LIMITS["output_bytes"] + 1)
        if len(build_out) > isolation.LIMITS["output_bytes"] or len(build_err) > isolation.LIMITS["output_bytes"]:
            print("build_output_limit", file=sys.stderr)
            return 100
        if exit_code:
            print("build_failed", file=sys.stderr)
            sys.stderr.buffer.write(build_err)
            return 100
    binary = Path("/scratch/program")
    if not binary.is_file() or binary.is_symlink() or not os.access(binary, os.X_OK):
        print("missing_build_output", file=sys.stderr)
        return 100
    try:
        isolation.install_runtime_seccomp()
        isolation.install_exec_landlock(binary)
    except isolation.UnsupportedIsolation as error:
        print(f"runtime_isolation_failed:{error}", file=sys.stderr)
        return 101
    print("cgx2_isolation_ready", file=sys.stderr, flush=True)
    os.execv(binary, [str(binary)])


if __name__ == "__main__":
    raise SystemExit(main())
