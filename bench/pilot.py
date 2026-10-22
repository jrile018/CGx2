"""Run the frozen six-pair CGx2/Python scalar parity and process-runtime pilot."""

import argparse
import hashlib
import json
import os
import platform
import re
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "bench/fixtures/pilot/v1/manifest.json"
TOKEN = re.compile(r'"(?:\\.|[^"\\])*"|[A-Za-z_][A-Za-z_0-9]*|[0-9]+|[^\s]', re.UNICODE)
ERROR_MARKERS = {"i64_overflow": {"cgx2": b"CGR005", "python": b"PILOT_I64_OVERFLOW"}}
CONDITIONS = ("cgx2", "python")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load_manifest(path=MANIFEST):
    path = Path(path).resolve()
    raw = path.read_bytes()
    data = json.loads(raw)
    if not isinstance(data, dict) or type(data.get("schema_version")) is not int or data["schema_version"] != 1:
        raise ValueError("pilot manifest schema_version must be 1")
    tasks = data.get("tasks")
    if not isinstance(tasks, list) or len(tasks) != 6:
        raise ValueError("pilot manifest must contain exactly six tasks")
    seen = set()
    for task in tasks:
        if not isinstance(task, dict) or set(task) != {"id", "cgx2", "python", "args", "oracle", "workload"}:
            raise ValueError("task fields do not match the pilot schema")
        task_id = task["id"]
        if not isinstance(task_id, str) or not re.fullmatch(r"[a-z][a-z0-9_]*", task_id) or task_id in seen:
            raise ValueError("task IDs must be unique lowercase identifiers")
        seen.add(task_id)
        if not isinstance(task["workload"], str) or not task["workload"]:
            raise ValueError(f"{task_id}: workload must be a nonempty description")
        for condition, suffix in (("cgx2", ".cg"), ("python", ".py")):
            name = task[condition]
            if not isinstance(name, str) or name != task_id + suffix:
                raise ValueError(f"{task_id}: {condition} fixture must match its task ID")
            if not (path.parent / name).is_file():
                raise ValueError(f"{task_id}: missing {condition} fixture")
        args = task["args"]
        if not isinstance(args, list) or any(not isinstance(x, str) or "\x00" in x for x in args):
            raise ValueError(f"{task_id}: args must be strings without NUL")
        oracle = task["oracle"]
        if not isinstance(oracle, dict) or set(oracle) != {"stdout", "exit_class", "error_class"}:
            raise ValueError(f"{task_id}: oracle fields do not match the pilot schema")
        if not isinstance(oracle["stdout"], str) or oracle["exit_class"] not in ("success", "error"):
            raise ValueError(f"{task_id}: invalid stdout or exit class")
        if oracle["exit_class"] == "success" and oracle["error_class"] is not None:
            raise ValueError(f"{task_id}: success cannot have an error class")
        if oracle["exit_class"] == "error" and oracle["error_class"] not in ERROR_MARKERS:
            raise ValueError(f"{task_id}: unknown error class")
    return path, raw, data


def source_size(path):
    raw = path.read_bytes()
    content = raw.decode("utf-8")
    return {"sha256": sha(raw), "utf8_bytes": len(raw), "characters": len(content),
            "lines": len(content.splitlines()), "lexical_tokens_descriptive": len(TOKEN.findall(content))}


def execute(argv, timeout, env=None):
    start = time.perf_counter()
    try:
        child = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 shell=False, start_new_session=True, env=env)
    except (FileNotFoundError, PermissionError) as error:
        return {"wall_seconds": time.perf_counter() - start, "exit_status": None,
                "stdout_sha256": None, "stderr_sha256": None, "stdout": b"", "stderr": str(error).encode(),
                "process_error": "missing_executable" if isinstance(error, FileNotFoundError) else "permission_denied"}
    try:
        stdout, stderr = child.communicate(timeout=timeout)
        process_error = None
        exit_status = child.returncode
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        stdout, stderr = child.communicate()
        process_error = "timeout"
        exit_status = None
    return {"wall_seconds": time.perf_counter() - start, "exit_status": exit_status,
            "stdout_sha256": sha(stdout), "stderr_sha256": sha(stderr),
            "stdout": stdout, "stderr": stderr, "process_error": process_error}


def assess(run, oracle, condition):
    reason = run["process_error"]
    if reason is None:
        actual_class = "success" if run["exit_status"] == 0 else "error"
        if actual_class != oracle["exit_class"]:
            reason = "exit_class_mismatch"
        elif run["stdout"] != oracle["stdout"].encode("utf-8"):
            reason = "stdout_mismatch"
        elif actual_class == "success" and run["stderr"]:
            reason = "unexpected_stderr"
        elif actual_class == "error" and ERROR_MARKERS[oracle["error_class"]][condition] not in run["stderr"]:
            reason = "error_class_mismatch"
    return reason


def record(run, reason):
    return {key: value for key, value in run.items() if key not in ("stdout", "stderr")} | {
        "valid": reason is None, "invalid_reason": reason,
        "stdout_text": run["stdout"].decode("utf-8", errors="replace")[:1000],
        "stderr_text": run["stderr"].decode("utf-8", errors="replace")[:1000]}


def command(condition, task, fixture_dir, cgx2, python):
    source = str(fixture_dir / task[condition])
    if condition == "cgx2":
        return [str(cgx2), "run", source, "--", *task["args"]]
    return [str(python), source, *task["args"]]


def parity(manifest_path, data, cgx2, python, timeout, env=None):
    cases = []
    for task in data["tasks"]:
        source = str(manifest_path.parent / task["cgx2"])
        check_run = execute([str(cgx2), "check", source], timeout, env)
        check_reason = check_run["process_error"]
        if check_reason is None and (check_run["exit_status"] != 0 or check_run["stdout"] or check_run["stderr"]):
            check_reason = "cgx2_check_failed"
        case = {"task_id": task["id"], "workload": task["workload"], "args": task["args"],
                "oracle": task["oracle"], "cgx2_check": record(check_run, check_reason), "runs": {},
                "source_size": {condition: source_size(manifest_path.parent / task[condition])
                                for condition in CONDITIONS}}
        for condition in CONDITIONS:
            if condition == "cgx2" and check_reason is not None:
                case["runs"][condition] = {"valid": False, "invalid_reason": "cgx2_check_failed", "skipped": True}
                continue
            run = execute(command(condition, task, manifest_path.parent, cgx2, python), timeout, env)
            case["runs"][condition] = record(run, assess(run, task["oracle"], condition))
        case["valid"] = all(case["runs"][condition]["valid"] for condition in CONDITIONS)
        cases.append(case)
    return {"valid": all(case["valid"] for case in cases), "cases": cases}


def percentile(values, fraction):
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def runtime(manifest_path, data, cgx2, python, timeout, warmups, repetitions, env, preflight):
    if not preflight["valid"]:
        return {"valid": False, "invalid_reason": "parity_preflight_failed", "samples": [], "summary": []}
    samples = []
    summaries = []
    for task in data["tasks"]:
        task_samples = []
        for pair_index in range(warmups + repetitions):
            phase = "warmup" if pair_index < warmups else "timed"
            # Alternate the first condition on every pair, including across tasks.
            order = CONDITIONS if (len(samples) // 2) % 2 == 0 else tuple(reversed(CONDITIONS))
            for position, condition in enumerate(order):
                run = execute(command(condition, task, manifest_path.parent, cgx2, python), timeout, env)
                sample = {"task_id": task["id"], "phase": phase, "pair_index": pair_index,
                          "condition": condition, "order": list(order), "order_position": position,
                          **record(run, assess(run, task["oracle"], condition))}
                samples.append(sample)
                task_samples.append(sample)
        task_summary = {"task_id": task["id"], "valid": all(x["valid"] for x in task_samples),
                        "conditions": {}}
        for condition in CONDITIONS:
            timed = [x["wall_seconds"] for x in task_samples if x["phase"] == "timed" and x["condition"] == condition]
            task_summary["conditions"][condition] = ({"n": len(timed), "median_seconds": statistics.median(timed),
                "p25_seconds": percentile(timed, 0.25), "p75_seconds": percentile(timed, 0.75),
                "iqr_seconds": percentile(timed, 0.75) - percentile(timed, 0.25)}
                if task_summary["valid"] else {"n": len(timed), "invalid": True})
        summaries.append(task_summary)
    valid = all(x["valid"] for x in samples)
    return {"valid": valid, "invalid_reason": None if valid else "runtime_sample_failed",
            "metric": "fresh_subprocess_wall_seconds_including_startup_excluding_compile",
            "warmups_per_condition": warmups, "timed_repetitions_per_condition": repetitions,
            "samples": samples, "summary": summaries}


def version(argv):
    run = execute(argv, 10)
    if run["process_error"] or run["exit_status"] != 0:
        return None
    return (run["stdout"] or run["stderr"]).decode("utf-8", errors="replace").strip()[:300]


def cpu_model():
    path = Path("/proc/cpuinfo")
    if path.exists():
        for line in path.read_text(errors="replace").splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    return platform.processor() or "unknown"


def provenance(manifest_raw, cgx2, python, build_seconds):
    return {"started_at_utc": datetime.now(timezone.utc).isoformat(),
            "host": platform.node(), "platform": platform.platform(), "cpu_model": cpu_model(),
            "logical_cpu_count": os.cpu_count(), "python_version": version([str(python), "--version"]),
            "rustc_version": version(["rustc", "--version"]), "cargo_version": version(["cargo", "--version"]),
            "cgx2_executable": str(cgx2), "cgx2_sha256": sha(cgx2.read_bytes()) if cgx2.is_file() else None,
            "python_executable": str(python), "python_sha256": sha(python.read_bytes()) if python.is_file() else None,
            "manifest_sha256": sha(manifest_raw), "build_wall_seconds": build_seconds,
            "thread_environment": {key: os.environ.get(key) for key in ("OMP_NUM_THREADS", "MKL_NUM_THREADS", "RAYON_NUM_THREADS")},
            "pythonhashseed": "0"}


def output_dir(path):
    if path is None:
        return Path(tempfile.mkdtemp(prefix="cgx2-pilot-", dir="/tmp"))
    path = Path(path).resolve()
    path.mkdir(parents=True, exist_ok=False)
    return path


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("parity", "runtime"))
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--cgx2", type=Path, help="existing CLI executable")
    source.add_argument("--build", action="store_true", help="run cargo build --offline -p cgx2-cli first")
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--out-dir", type=Path, help="new output directory; default: unique directory under /tmp")
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--warmups", type=int, default=2)
    parser.add_argument("--repetitions", type=int, default=10)
    args = parser.parse_args(argv)
    if not 0 < args.timeout <= 3600 or args.warmups < 1 or args.repetitions < 10:
        parser.error("timeout must be 0 < t <= 3600, warmups >= 1, repetitions >= 10")
    try:
        manifest_path, manifest_raw, data = load_manifest(args.manifest)
        directory = output_dir(args.out_dir)
        build_seconds = None
        if args.build:
            start = time.perf_counter()
            result = subprocess.run(["cargo", "build", "--offline", "-p", "cgx2-cli"], cwd=ROOT,
                                    shell=False, capture_output=True, text=True, timeout=600, check=False)
            build_seconds = time.perf_counter() - start
            if result.returncode != 0:
                raise ValueError("cargo build failed: " + result.stderr[-2000:])
            cgx2 = ROOT / "target/debug/cgx2"
        else:
            cgx2 = args.cgx2.resolve()
        python = args.python.resolve()
        env = os.environ.copy()
        env["PYTHONHASHSEED"] = "0"
        run_provenance = provenance(manifest_raw, cgx2, python, build_seconds)
        preflight = parity(manifest_path, data, cgx2, python, args.timeout, env)
        timed = (runtime(manifest_path, data, cgx2, python, args.timeout, args.warmups,
                         args.repetitions, env, preflight) if args.mode == "runtime" else None)
        executable_drift = ((sha(cgx2.read_bytes()) if cgx2.is_file() else None) != run_provenance["cgx2_sha256"]
                            or (sha(python.read_bytes()) if python.is_file() else None) != run_provenance["python_sha256"])
        fixture_drift = sha(manifest_path.read_bytes()) != run_provenance["manifest_sha256"]
        for case in preflight["cases"]:
            task = next(task for task in data["tasks"] if task["id"] == case["task_id"])
            for condition in CONDITIONS:
                path = manifest_path.parent / task[condition]
                fixture_drift |= (sha(path.read_bytes()) if path.is_file() else None) != case["source_size"][condition]["sha256"]
        if executable_drift or fixture_drift:
            preflight["valid"] = False
            preflight["invalid_reason"] = "input_changed_during_run"
            if timed is not None:
                timed["valid"] = False
                timed["invalid_reason"] = "input_changed_during_run"
        result = {"schema_version": 1, "kind": "cgx2_scalar_pilot", "pilot_only": True,
                  "scope": data.get("scope"), "provenance": run_provenance,
                  "timeout_seconds": args.timeout, "parity": preflight,
                  "runtime": timed,
                  "llm_trace_metrics": None, "release_conclusion": None}
        target = directory / ("runtime.json" if args.mode == "runtime" else "parity.json")
        target.write_text(json.dumps(result, indent=2, sort_keys=True, ensure_ascii=False) + "\n", encoding="utf-8")
        print(target)
        return 0 if preflight["valid"] and (result["runtime"] is None or result["runtime"]["valid"]) else 1
    except (ValueError, OSError, json.JSONDecodeError, subprocess.TimeoutExpired) as error:
        parser.exit(2, f"pilot setup error: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
