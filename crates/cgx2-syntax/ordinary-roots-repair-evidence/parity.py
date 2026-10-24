#!/usr/bin/env python3
"""Incremental frozen-artifact checks for OM05/06/07 and retained other-lane defects."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
spec = importlib.util.spec_from_file_location("ordinary_runner", HERE.parent / "ordinary-memory-evidence/run.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--outside-dir", type=Path, required=True)
    args = parser.parse_args()
    binary, out, outside = args.binary_dir.resolve(), args.output_dir.resolve(), args.outside_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    outside.mkdir(parents=True, exist_ok=False)
    assert not outside.is_relative_to(ROOT)
    manifest = json.loads((binary.parent / "frozen.json").read_text())
    frozen = dict(manifest["binary_sha256"], **manifest["probe_library_sha256"])
    report = {"schema_version": 1, "commands": [], "checks": [], "known_unrepaired": [],
              "outside_repository": str(outside), "ordinary_memory_accepted": False,
              "limits": {"wall_seconds": 180, "cpu_seconds_per_process": 120,
                         "address_bytes_per_process": 4 * 1024**3,
                         "file_bytes": 512 * 1024**2, "retained_bytes_per_stream": 2 * 1024**2},
              "source_sha256": {}, "frozen_before": frozen}
    env = dict(os.environ, RUSTUP_TOOLCHAIN="1.98.1", CGX2_MEMORY_EVIDENCE="1", PYTHONDONTWRITEBYTECODE="1")

    def save():
        report["passed"] = sum(c["passed"] for c in report["checks"])
        report["failed"] = sum(not c["passed"] for c in report["checks"])
        (out / "parity.json").write_text(json.dumps(report, indent=2) + "\n")

    def check(name, passed):
        report["checks"].append({"name": name, "passed": bool(passed)})
        save()

    def run(name, argv, cwd=ROOT):
        result = runner.supervise([str(a) for a in argv], cwd=str(cwd), env=env,
                                  limits=runner.limits, cap=2 * 1024**2, timeout=180)
        row = dict(name=name, argv=[str(a) for a in argv], cwd=str(cwd), **runner.evidence(result))
        report["commands"].append(row)
        save()
        return row

    wrapper = out / "rustc-two-threads"
    wrapper.write_text('#!/bin/sh\nexec rustc -C link-arg=-Wl,--threads=2 "$@"\n')
    wrapper.chmod(0o700)
    check("initial-frozen-hashes", all(digest(binary / k) == v for k, v in frozen.items()))
    repro = ROOT / "plans/release-completion/s2b-ordinary-memory-review/repros"
    nested = (HERE / "fixtures/nested-assignment.cg").read_text()
    copied = (HERE.parent / "ordinary-memory-evidence/fixtures/copy-cycle.cg").read_text()
    fixtures = {"outer-assignment": ((repro / "outer-root-assignment.cg").read_text(), "1\n", 0),
                "copy-cycle": (copied, "42\n", 0),
                "copy-failed": (copied.replace("limit: 16384", "limit: 0"), "-2\n", 0),
                "nested-assignment": (nested, "42\n", 0),
                "nested-return": (nested.replace("total = 42", "return 42"), "42\n", 0),
                "nested-error": (nested.replace("total = 42", "total = 42 / 0"), "", 1)}
    for name, (source, expected, exit_code) in fixtures.items():
        path = out / (name + ".cg")
        path.write_text(source)
        report["source_sha256"][name] = digest(path)
        interpreted = run(name + "-interpreter", [binary / "cgx2", "run", path])
        check(name + "-interpreter-value", runner.normal(interpreted, exit_code) and interpreted["stdout"] == expected)
        observation = runner.observed(interpreted)
        check(name + "-root-deltas", observation is not None
              and observation["roots_registered"] == observation["roots_released"]
              and observation["baseline_roots"] == observation["final_roots"] == 0
              and observation["cleanup_failures"] == 0)
        emitted = outside / (name + ".rs")
        emission = run(name + "-emit", [binary / "cgx2c", "emit-rust", path, "--out", emitted])
        check(name + "-emit", runner.normal(emission))
        for mode in ("debug", "release", "standalone"):
            artifact = outside / (name + "-" + mode)
            if mode == "standalone":
                argv = [wrapper, "--edition=2021", emitted, "-o", artifact]
            else:
                argv = [binary / "cgx2c", "build", path, "--out", artifact, "--rustc", wrapper]
                if mode == "release":
                    argv.append("--release")
            built = run(name + "-" + mode + "-build", argv, cwd=outside)
            check(name + "-" + mode + "-build", runner.normal(built))
            if runner.normal(built):
                actual = run(name + "-" + mode + "-run", [artifact], cwd=outside)
                check(name + "-" + mode + "-value", runner.normal(actual, exit_code) and actual["stdout"] == expected)
                check(name + "-" + mode + "-memory", runner.stable(runner.observed(actual)) == runner.stable(observation))
                if exit_code:
                    start = source.index("42 / 0")
                    span = f"{start}..{start + 6}"
                    check(name + "-" + mode + "-error", "CGR006" in actual["stderr"] and span in actual["stderr"] and span in interpreted["stderr"])

    # The reviewed exact direct safe-API probe uses the canonical top-level library.
    probe = out / "copy-root.rs"
    probe.write_bytes((repro / "copy-root.rs").read_bytes())
    report["source_sha256"][probe.name] = digest(probe)
    built = run("canonical-copy-root-build", [wrapper, "--edition=2021", probe, "--extern",
                f"cgx2_runtime={binary / 'libcgx2_runtime.rlib'}", "-L", f"dependency={binary / 'deps'}", "-o", out / "copy-root"])
    check("canonical-copy-root-build", runner.normal(built))
    if runner.normal(built):
        actual = run("canonical-copy-root-run", [out / "copy-root"])
        check("canonical-copy-root-source-survives", runner.normal(actual)
              and "source=Ok([42, 0, 0, 0, 0, 0, 0, 0]), copied=Ok([42, 0, 0, 0, 0, 0, 0, 0])" in actual["stdout"])

    # Retain product failures owned by the other lanes without treating them as acceptance.
    for defect, file, operation in [("OM01", "shadow-collect", "check"), ("OM02", "legacy-guard-panic.cg", "check"),
                                   ("OM03", "user-err-region-escape.cg", "check"), ("OM04", "user-borrow.cg", "check"),
                                   ("OM08", "api-private.cg", "api")]:
        path = repro / file
        for p in sorted(path.rglob("*")) if path.is_dir() else [path]:
            if p.is_file():
                report["source_sha256"][str(p.relative_to(repro))] = digest(p)
        argv = [binary / "cgx2", operation, path] + (["--json"] if operation == "api" else [])
        actual = run(defect + "-unrepaired", argv)
        report["known_unrepaired"].append({"id": defect, "status": "failed", "command": actual["name"],
            "exit": actual["exit"], "reason": actual["reason"], "signal": actual.get("signal")})
        save()
    report["frozen_after"] = {k: digest(binary / k) for k in frozen}
    check("final-frozen-hashes", report["frozen_after"] == frozen)
    check("dependency-hashes", all(digest(binary / "deps" / k) == v for k, v in manifest["dependency_sha256"].items()))
    print(json.dumps({k: report[k] for k in ("passed", "failed", "known_unrepaired")}))
    return bool(report["failed"])


if __name__ == "__main__":
    sys.exit(main())
