#!/usr/bin/env python3
"""Bounded source/SMT growth check; this is finite parity, not solver acceptance."""
import hashlib
import json
import pathlib
import resource
import subprocess
import sys
import tempfile


def cap_child():
    resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (15, 15))


def main():
    verifier = str(pathlib.Path(sys.argv[1]).resolve())
    rows = []
    for depth in [1, 2, 3, 4, 8, 16, 32, 64, 140]:
        source = ("pub fn f(x: i64) -> i64 effects() caps():\n"
                  "    requires true\n    ensures result == x\n    return x"
                  + " / 1" * depth + "\n")
        with tempfile.TemporaryDirectory(prefix="cgx2-size-") as work:
            path = pathlib.Path(work) / "division.cg"
            path.write_text(source)
            command = [verifier, str(path), "--domain", "x:-1:1", "--format", "json"]
            run = subprocess.run(command, capture_output=True, text=True, timeout=15,
                                 check=True, preexec_fn=cap_child)
            report = json.loads(run.stdout)
        script = report.get("smtlib")
        if depth == 140:
            assert report["diagnostic"]["code"] == "CGV003", report
            assert report["solver"]["launched"] is False, report
            assert script is None, report
        else:
            assert report["bounded_solver_result"]["kind"] == "cross_checked", report
            assert report["bounded_solver_result"]["source_parity"] is True, report
            assert report["bounded_solver_result"]["checked_cases"] == 3, report
            assert len(script) < 3000 * depth + 4000
        assert report["proof"]["kind"] == "unknown", report
        rows.append({"division_depth": depth, "source_bytes": len(source.encode()),
                     "smt_bytes": len(script.encode()) if script else None,
                     "smt_sha256": hashlib.sha256(script.encode()).hexdigest() if script else None,
                     "audit": report["bounded_solver_result"], "proof": report["proof"]["kind"],
                     "diagnostic": report.get("diagnostic")})
    print(json.dumps({"schema_version": 1, "memory_limit_bytes": 512 * 1024 * 1024,
                      "cpu_limit_seconds": 15, "timeout_seconds": 15,
                      "solver_acceptance": "unrun", "rows": rows}, indent=2))


if __name__ == "__main__":
    main()
