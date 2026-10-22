"""Version 1 native scalar parity and fresh-process timing pilot (no model claims)."""

import argparse
import hashlib
import json
import os
import platform
import random
import statistics
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

import pilot


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "bench/fixtures/native/v1/manifest.json"
CONDITIONS = ("cgx2", "rust")
ERROR_MARKERS = {"i64_overflow": b"i64 arithmetic overflow"}


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate_cases(cases, fixture_dir):
    if not isinstance(cases, list) or not cases:
        raise ValueError("native manifest needs at least one case")
    seen = set()
    for case in cases:
        if not isinstance(case, dict) or set(case) != {"id", "cgx2", "rust", "args", "oracle"}:
            raise ValueError("native case fields are invalid")
        if not isinstance(case["id"], str) or case["id"] in seen:
            raise ValueError("native case IDs must be unique strings")
        seen.add(case["id"])
        for condition, extension in (("cgx2", ".cg"), ("rust", ".rs")):
            name = case[condition]
            if not isinstance(name, str) or not name.endswith(extension) or not (fixture_dir / name).is_file():
                raise ValueError(f"{case['id']}: missing {condition} reference")
        if not isinstance(case["args"], list) or any(not isinstance(x, str) for x in case["args"]):
            raise ValueError(f"{case['id']}: args must be strings")
        oracle = case["oracle"]
        if not isinstance(oracle, dict) or set(oracle) != {"stdout", "exit_status", "exit_class", "error_class"}:
            raise ValueError(f"{case['id']}: oracle is invalid")
        if (not isinstance(oracle["stdout"], str) or oracle["exit_class"] not in ("success", "error")
                or type(oracle["exit_status"]) is not int or
                (oracle["exit_class"] == "success") != (oracle["exit_status"] == 0)):
            raise ValueError(f"{case['id']}: oracle is invalid")
        if (oracle["exit_class"] == "success" and oracle["error_class"] is not None
                or oracle["exit_class"] == "error" and oracle["error_class"] not in ERROR_MARKERS):
            raise ValueError(f"{case['id']}: error class is invalid")


def load_manifest(path=MANIFEST):
    path = Path(path).resolve()
    raw = path.read_bytes()
    data = json.loads(raw)
    if not isinstance(data, dict) or data.get("schema_version") != 1 or data.get("kind") != "native_scalar_pilot":
        raise ValueError("native pilot requires schema_version 1 and kind native_scalar_pilot")
    validate_cases(data.get("cases"), path.parent)
    return path, raw, data


def run_binary(binary, args, timeout):
    return pilot.execute([str(binary), *args], timeout)


def median_interval(values):
    rng = random.Random(0)
    medians = [statistics.median(rng.choices(values, k=len(values))) for _ in range(2000)]
    return [pilot.percentile(medians, .025), pilot.percentile(medians, .975)]


def assess(run, oracle):
    if run["process_error"]:
        return run["process_error"]
    actual = "success" if run["exit_status"] == 0 else "error"
    if actual != oracle["exit_class"]:
        return "exit_class_mismatch"
    if run["exit_status"] != oracle["exit_status"]:
        return "exit_status_mismatch"
    if run["stdout"] != oracle["stdout"].encode("utf-8"):
        return "stdout_mismatch"
    if actual == "success" and run["stderr"]:
        return "unexpected_stderr"
    if actual == "error" and ERROR_MARKERS[oracle["error_class"]] not in run["stderr"]:
        return "error_class_mismatch"
    return None


def build_cases(cases, fixture_dir, output_dir, cgx2c, rustc, timeout):
    output_dir.mkdir(parents=True, exist_ok=True)
    records = []
    binaries = {}
    for case in cases:
        builds = {}
        paths = {}
        for condition in CONDITIONS:
            source = (fixture_dir / case[condition]).resolve()
            binary = output_dir / f"{case['id']}-{condition}"
            command = ([str(cgx2c), "build", str(source), "--out", str(binary), "--release", "--rustc", str(rustc)]
                       if condition == "cgx2" else
                       [str(rustc), "--edition=2024", str(source), "-o", str(binary), "-O"])
            run = pilot.execute(command, timeout)
            reason = run["process_error"] or ("compiler_failure" if run["exit_status"] != 0 else None)
            if reason is None and not binary.is_file():
                reason = "missing_build_output"
            builds[condition] = {"command": command, "profile": "rustc -O, edition 2024",
                                 "source_sha256": digest(source), "binary_sha256": digest(binary) if binary.is_file() else None,
                                 **pilot.record(run, reason)}
            if reason is None:
                paths[condition] = binary
        records.append({"task_id": case["id"], **builds})
        if len(paths) == len(CONDITIONS):
            binaries[case["id"]] = paths
    return {"valid": all(all(item[c]["valid"] for c in CONDITIONS) for item in records), "cases": records}, binaries


def parity(cases, fixture_dir, binaries, timeout):
    records = []
    for case in cases:
        runs = {}
        for condition in CONDITIONS:
            binary = binaries.get(case["id"], {}).get(condition)
            if binary is None:
                runs[condition] = {"valid": False, "invalid_reason": "build_failed", "skipped": True}
                continue
            run = run_binary(binary, case["args"], timeout)
            runs[condition] = pilot.record(run, assess(run, case["oracle"]))
        records.append({"task_id": case["id"], "args": case["args"], "oracle": case["oracle"],
                        "runs": runs, "valid": all(runs[c]["valid"] for c in CONDITIONS)})
    return {"valid": all(item["valid"] for item in records), "cases": records,
            "phase": "first_subprocess_launch_after_build_cache_uncontrolled"}


def timing(cases, fixture_dir, binaries, timeout, warmups, repetitions, preflight):
    if not preflight["valid"]:
        return {"valid": False, "invalid_reason": "parity_preflight_failed", "samples": [], "summary": []}
    samples = []
    summary = []
    pair_serial = 0
    for case in cases:
        case_samples = []
        for pair_index in range(warmups + repetitions):
            phase = "warmup" if pair_index < warmups else "timed"
            order = CONDITIONS if pair_serial % 2 == 0 else tuple(reversed(CONDITIONS))
            pair_serial += 1
            for position, condition in enumerate(order):
                run = run_binary(binaries[case["id"]][condition], case["args"], timeout)
                item = {"task_id": case["id"], "phase": phase, "pair_index": pair_index,
                        "condition": condition, "order": list(order), "order_position": position,
                        **pilot.record(run, assess(run, case["oracle"]))}
                samples.append(item)
                case_samples.append(item)
        valid = all(item["valid"] for item in case_samples)
        conditions = {}
        for condition in CONDITIONS:
            values = [item["wall_seconds"] for item in case_samples
                      if item["phase"] == "timed" and item["condition"] == condition]
            conditions[condition] = ({"n": len(values), "median_seconds": statistics.median(values),
                                      "p25_seconds": pilot.percentile(values, .25),
                                      "p75_seconds": pilot.percentile(values, .75),
                                      "median_bootstrap_95_seconds": median_interval(values),
                                      "bootstrap_resamples": 2000, "bootstrap_seed": 0} if valid else
                                     {"n": len(values), "invalid": True})
        summary.append({"task_id": case["id"], "valid": valid, "conditions": conditions})
    valid = all(item["valid"] for item in samples)
    return {"valid": valid, "invalid_reason": None if valid else "timing_sample_failed",
            "metric": "fresh_subprocess_wall_seconds_including_startup_excluding_build",
            "warmups_per_condition": warmups, "timed_repetitions_per_condition": repetitions,
            "samples": samples, "summary": summary}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--cgx2c", type=Path, default=ROOT / "target/debug/cgx2c")
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument("--timeout", type=float, default=10)
    parser.add_argument("--build-timeout", type=float, default=120)
    parser.add_argument("--warmups", type=int, default=2)
    parser.add_argument("--repetitions", type=int, default=10)
    parser.add_argument("--out", type=Path, help="new JSON output file")
    args = parser.parse_args(argv)
    if args.timeout <= 0 or args.build_timeout <= 0 or args.warmups < 1 or args.repetitions < 2:
        parser.error("timeouts must be positive, warmups >= 1, repetitions >= 2")
    try:
        path, raw, data = load_manifest(args.manifest)
        if args.out:
            destination = args.out
        else:
            descriptor, name = tempfile.mkstemp(prefix="cgx2-native-pilot-", suffix=".json", dir="/tmp")
            os.close(descriptor)
            destination = Path(name)
        if args.out and destination.exists():
            raise FileExistsError(destination)
        with tempfile.TemporaryDirectory(prefix="cgx2-native-build-") as temp:
            builds, binaries = build_cases(data["cases"], path.parent, Path(temp), args.cgx2c,
                                            args.rustc, args.build_timeout)
            preflight = parity(data["cases"], path.parent, binaries, args.timeout)
            timed = timing(data["cases"], path.parent, binaries, args.timeout,
                           args.warmups, args.repetitions, preflight)
            result = {"schema_version": 1, "kind": "native_scalar_pilot", "pilot_only": True,
                      "scope": "six tiny scalar programs; subprocess startup included; no managed/owned runtime or LLM claims",
                      "manifest_sha256": hashlib.sha256(raw).hexdigest(),
                      "provenance": {"started_at_utc": datetime.now(timezone.utc).isoformat(),
                                     "host": platform.node(), "platform": platform.platform(),
                                     "cpu_model": pilot.cpu_model(), "logical_cpu_count": os.cpu_count(),
                                     "thread_environment": {k: os.environ.get(k) for k in
                                                            ("OMP_NUM_THREADS", "MKL_NUM_THREADS", "RAYON_NUM_THREADS")},
                                     "rustc_version": pilot.version([args.rustc, "--version"]),
                                     "cgx2c_sha256": digest(args.cgx2c) if args.cgx2c.is_file() else None},
                      "builds": builds,
                      "cold_launch": {"qualification": "first subprocess launch after build; OS cache state uncontrolled",
                                      "cases": [{"task_id": item["task_id"],
                                                 "wall_seconds": {c: item["runs"][c].get("wall_seconds") for c in CONDITIONS}}
                                                for item in preflight["cases"]]},
                      "parity": preflight, "timing": timed,
                      "llm_trace_metrics": None, "release_conclusion": None}
        destination.write_text(json.dumps(result, indent=2, sort_keys=True, ensure_ascii=False) + "\n")
        print(destination)
        return 0 if builds["valid"] and preflight["valid"] and timed["valid"] else 1
    except (ValueError, OSError, json.JSONDecodeError) as error:
        parser.exit(2, f"native pilot setup error: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
