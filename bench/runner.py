"""On-demand, stdlib-only execution of one benchmark manifest."""

import argparse
import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path


SHELL_SYNTAX = re.compile(r"[;&|<>`]|\$\(")
SECRET_NAME = re.compile(r"SECRET|TOKEN|PASSWORD|PASSWD|KEY|CREDENTIAL|PRIVATE|AUTH|ACCESS|SESSION|COOKIE|(^|_)PAT($|_)", re.I)
TIERS = {"small", "large"}
SHELLS = {"sh", "bash", "zsh", "fish", "dash", "ksh", "cmd", "powershell", "pwsh"}


def _argv(value, name):
    if not isinstance(value, list) or not value or any(
        not isinstance(part, str) or not part or "\x00" in part or SHELL_SYNTAX.search(part)
        for part in value
    ) or (isinstance(value, list) and value and isinstance(value[0], str)
          and Path(value[0]).stem.lower() in SHELLS):
        raise ValueError(f"{name} must be a nonempty argv array without shell syntax")
    return value


def _manifest(data):
    if not isinstance(data, dict):
        raise ValueError("manifest must be an object")
    for key in ("task_id", "language", "variant", "model_id", "cwd"):
        if not isinstance(data.get(key), str) or not data[key]:
            raise ValueError(f"{key} must be a nonempty string")
    if data.get("tier") not in TIERS:
        raise ValueError("tier must be small or large")
    _argv(data.get("argv"), "argv")
    _argv(data.get("hidden_test_argv"), "hidden_test_argv")
    if type(data.get("timeout_seconds")) not in (int, float) or not 0 < data["timeout_seconds"] <= 86400:
        raise ValueError("timeout_seconds must be positive and at most 86400")
    for key in ("seed", "expected_exit"):
        if type(data.get(key)) is not int:
            raise ValueError(f"{key} must be an integer")
    if not isinstance(data.get("budget"), dict) or type(data["budget"].get("total_tokens")) is not int or data["budget"]["total_tokens"] <= 0:
        raise ValueError("budget.total_tokens must be a positive integer")
    for key in ("versions", "environment"):
        if not isinstance(data.get(key), dict) or any(not isinstance(k, str) or not isinstance(v, str) for k, v in data[key].items()):
            raise ValueError(f"{key} must map strings to strings")
    if "trace_path" in data and (not isinstance(data["trace_path"], str) or not data["trace_path"]):
        raise ValueError("trace_path must be a path string")
    return data


def _sha(output):
    return hashlib.sha256(output).hexdigest()


def _execute(argv, cwd, env, timeout):
    try:
        child = subprocess.Popen(argv, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, shell=False, start_new_session=True)
    except FileNotFoundError:
        return {"exit": None, "stdout": None, "stderr": None, "reason": "missing_executable"}
    try:
        stdout, stderr = child.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        stdout, stderr = child.communicate()
        return {"exit": None, "stdout": _sha(stdout), "stderr": _sha(stderr), "reason": "timeout"}
    return {"exit": child.returncode, "stdout": _sha(stdout), "stderr": _sha(stderr), "reason": None}


def _trace(path):
    if path is None or not path.exists():
        return {key: None for key in ("output_tokens", "total_tokens", "iterations", "human_interventions")}
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("trace must be an object")
    values = {}
    for key in ("output_tokens", "total_tokens", "iterations", "human_interventions"):
        value = data.get(key)
        if value is not None and (type(value) is not int or value < 0):
            raise ValueError(f"trace {key} must be a nonnegative integer or null")
        values[key] = value
    if values["output_tokens"] is not None and values["total_tokens"] is not None and values["output_tokens"] > values["total_tokens"]:
        raise ValueError("output_tokens cannot exceed total_tokens")
    return values


def run_manifest(path):
    path = Path(path).resolve()
    raw = path.read_bytes()
    manifest = _manifest(json.loads(raw))
    cwd = (path.parent / manifest["cwd"]).resolve()
    if not cwd.is_dir():
        raise ValueError("cwd must name an existing directory")
    env = {"PATH": os.environ.get("PATH", os.defpath), "LANG": "C.UTF-8"}
    env.update(manifest["environment"])
    trace = _trace((path.parent / manifest["trace_path"]).resolve() if "trace_path" in manifest else None)
    started = datetime.now(timezone.utc).isoformat()
    begin = time.monotonic()
    first = _execute(manifest["argv"], cwd, env, manifest["timeout_seconds"])
    hidden = None
    reason = first["reason"]
    if reason is None and first["exit"] != manifest["expected_exit"]:
        reason = "unexpected_exit"
    if reason is None:
        remaining = manifest["timeout_seconds"] - (time.monotonic() - begin)
        hidden = _execute(manifest["hidden_test_argv"], cwd, env, max(remaining, 0.001)) if remaining > 0 else None
        if hidden is None or hidden["reason"] == "timeout":
            reason = "timeout"
        elif hidden["reason"]:
            reason = hidden["reason"]
        elif hidden["exit"] != 0:
            reason = "hidden_test_failed"
    elapsed = time.monotonic() - begin
    return {
        "schema_version": 1, "task_id": manifest["task_id"], "tier": manifest["tier"],
        "language": manifest["language"], "variant": manifest["variant"],
        "model_id": manifest["model_id"], "seed": manifest["seed"],
        "timeout_seconds": manifest["timeout_seconds"], "budget_total_tokens": manifest["budget"]["total_tokens"],
        "wall_seconds": elapsed, "charged_wall_seconds": manifest["timeout_seconds"] if reason else elapsed,
        "exit_status": first["exit"], "stdout_sha256": first["stdout"], "stderr_sha256": first["stderr"],
        "hidden_test_exit_status": hidden["exit"] if hidden else None,
        "hidden_test_stdout_sha256": hidden["stdout"] if hidden else None,
        "hidden_test_stderr_sha256": hidden["stderr"] if hidden else None,
        "verified_completion": reason is None, "failure_reason": reason,
        **trace,
        "provenance": {"started_at_utc": started, "manifest_sha256": _sha(raw),
                       "versions": {k: v for k, v in manifest["versions"].items() if not SECRET_NAME.search(k)},
                       "environment": {k: v for k, v in env.items() if not SECRET_NAME.search(k)},
                       "runner_python": sys.version.split()[0]},
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        result = run_manifest(args.manifest)
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (ValueError, OSError, json.JSONDecodeError) as error:
        parser.exit(2, f"benchmark manifest error: {error}\n")


if __name__ == "__main__":
    main()
