#!/usr/bin/env python3
"""Bounded Result equality parity on interpreter, native profiles and standalone Rust."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shlex
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(HERE.parent / "owned-result-runner-evidence"))
from supervisor import supervise, evidence


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def stable(observation):
    result = json.loads(json.dumps(observation))
    for region in result["memory"]["regions"]:
        region["region_id"]["session"] = 0
        region.pop("pause")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary_dir.resolve() / "cgx2"
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "product.json"
    if report_path.exists():
        parser.error("use a fresh output directory to retain previous attempts")
    records = []
    report = {"schema_version": 1, "checks": records, "release_complete": False,
              "managed_delivery_accepted": False, "external_leak_audit_performance": "unrun",
              "binary_sha256": digest(binary), "runner_sha256": digest(Path(__file__)),
              "runtime_graph_sha256": digest(ROOT / "crates/cgx2-runtime/src/graph.rs"),
              "bounds": {"wall_seconds": 180, "cpu_seconds_per_process": 120,
                  "address_space_bytes_per_process": 4 * 1024**3,
                  "retained_bytes_per_stream": 2 * 1024**2,
                  "aggregate_cgroup_isolation": False}, "sources_sha256": {}}
    env = {**os.environ, "RUSTUP_TOOLCHAIN": "1.98.1", "CGX2_MANAGED_EVIDENCE": "1"}

    def save():
        report["passed"] = sum(r["passed"] for r in records)
        report["failed"] = sum(not r["passed"] for r in records)
        report_path.write_text(json.dumps(report, indent=2) + "\n")

    def limits():
        resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
        resource.setrlimit(resource.RLIMIT_CPU, (120, 120))
        resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2, 512 * 1024**2))

    def command(label, argv, cwd):
        result = supervise([str(a) for a in argv], cwd=cwd, env=env,
                           limits=limits, cap=2 * 1024**2, timeout=180)
        record = {"label": label, "command": [str(a) for a in argv], "cwd": str(cwd),
                  **evidence(result)}
        record["passed"] = result["reason"] == "exit" and result["exit"] == 0
        records.append(record)
        save()
        if not record["passed"]:
            raise RuntimeError(label)
        return record

    def assertion(label, condition):
        records.append({"label": label, "passed": bool(condition)})
        save()
        if not condition:
            raise RuntimeError(label)

    def accounting(record):
        rows = [line.split(" ", 1)[1] for line in record["stderr"].splitlines()
                if line.startswith("CGX2_MANAGED_EVIDENCE ")]
        assertion(record["label"] + ":one-memory-observation", len(rows) == 1)
        return json.loads(rows[0])

    with tempfile.TemporaryDirectory(prefix="result-equality-", dir=os.environ["TMPDIR"]) as temporary:
        work = Path(temporary)
        pinned = command("pinned-rustc", ["rustup", "which", "rustc"], work)["stdout"].strip()
        identity = command("rustc-identity", [pinned, "-vV"], work)
        assertion("pinned-version", "release: 1.98.1" in identity["stdout"])
        wrapper = work / "rustc-linker"
        wrapper.write_text("#!/bin/sh\nexec " + shlex.quote(pinned) + ' -Clink-arg=-Wl,--threads=2 "$@"\n')
        wrapper.chmod(0o700)
        for name, failures in [("ok", 0), ("err", 2), ("nested", 0), ("nested-err", 1)]:
            source = HERE / (name + ".cg")
            report["sources_sha256"][name] = digest(source)
            command(name + ":check", [binary, "check", source], work)
            interpreted = command(name + ":interpreter", [binary, "run", source], work)
            observed = accounting(interpreted)
            assertion(name + ":value", interpreted["stdout"] == "42\n")
            assertion(name + ":real-recovery", observed["recovered"] == failures)
            assertion(name + ":root-cleanup", observed["final_roots"] == observed["baseline_roots"] == 0
                      and observed["roots_registered"] == observed["roots_released"]
                      and observed["cleanup_failures"] == 0)
            assertion(name + ":no-guard-collection", observed["memory"]["totals"]["collection_count"] == 0)
            for release in [False, True]:
                label = name + ("-release" if release else "-debug")
                artifact = work / label
                command(label + ":build", [binary, "build", source, "--out", artifact,
                        "--rustc", wrapper] + (["--release"] if release else []), work)
                native = command(label + ":execute", [artifact], work)
                assertion(label + ":value-and-memory-parity", native["stdout"] == interpreted["stdout"]
                          and stable(accounting(native)) == stable(observed))
                report.setdefault("artifacts_sha256", {})[label] = digest(artifact)
            emitted = work / (name + ".rs")
            artifact = work / (name + "-standalone")
            command(name + ":emit", [binary, "emit", source, "--target", "rust", "--out", emitted], work)
            text = emitted.read_text()
            assertion(name + ":standalone-embedded-runtime", "include!(" not in text
                      and "include_str!(" not in text and "#[path" not in text
                      and "extern crate" not in text)
            command(name + ":standalone-compile", [wrapper, "--edition=2024", emitted, "-o", artifact], work)
            standalone = command(name + ":standalone-execute", [artifact], work)
            assertion(name + ":standalone-parity", standalone["stdout"] == interpreted["stdout"]
                      and stable(accounting(standalone)) == stable(observed))
            report["artifacts_sha256"][emitted.name] = digest(emitted)
            report["artifacts_sha256"][artifact.name] = digest(artifact)
            save()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
