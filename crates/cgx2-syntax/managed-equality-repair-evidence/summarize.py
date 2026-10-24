#!/usr/bin/env python3
"""Verify retained final evidence and hash the bounded repair inputs."""
import gzip
import hashlib
import json
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
BASE = "3c1cfe2aeba2f7135e4069709bf633c082d2aab0"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    evidence = HERE / "evidence"
    gates = {}
    for label in ["build", "affected-tests", "typed-negative-corrected", "final-clippy",
                  "final-fmt", "final-diff", "equality-product-final", "managed-product",
                  "graph99-recheck", "guards518-recheck", "reports265-recheck"]:
        record = json.loads((evidence / (label + ".json")).read_text())
        raw = gzip.decompress((evidence / record["log"]).read_bytes())
        assert record["log_sha256"] == digest(raw), label
        assert record["passed"] and record["exit"] == 0 and record["reason"] == "exit", label
        assert record["signal"] is None and record["overflow_stream"] is None, label
        assert record["runtime_graph_sha256"] == digest((ROOT / "crates/cgx2-runtime/src/graph.rs").read_bytes()), label
        gates[label] = {"status": "passed", "record": "evidence/" + label + ".json"}
    for name, count in [("parity-final/product", 87), ("managed-product/product", 328),
                        ("graph99-product", 99), ("guards518-product", 518), ("reports265-product", 265)]:
        record = json.loads((evidence / (name + ".json")).read_text())
        assert record["passed"] == count and all(row["passed"] for row in record["checks"]), name
        if name == "parity-final/product":
            assert record["runner_sha256"] == digest((HERE / "run.py").read_bytes())
            assert record["runtime_graph_sha256"] == digest((ROOT / "crates/cgx2-runtime/src/graph.rs").read_bytes())
            for fixture, sha in record["sources_sha256"].items():
                assert sha == digest((HERE / (fixture + ".cg")).read_bytes()), fixture
        gates[name] = {"status": "passed", "checks": count, "record": "evidence/" + name + ".json"}
    manifests = {}
    for name in ["Cargo.toml", "Cargo.lock"]:
        current = digest((ROOT / name).read_bytes())
        original = digest(subprocess.check_output(["git", "show", BASE + ":" + name], cwd=ROOT))
        assert current == original, name
        manifests[name] = {"sha256": current, "unchanged_from_base": True}
    paths = [ROOT / "crates/cgx2-runtime/src/graph.rs",
             ROOT / "crates/cgx2-runtime/tests/managed_result_equality.rs",
             ROOT / "crates/cgx2-sema/tests/managed_result_equality.rs",
             ROOT / "crates/cgx2-syntax/MANAGED-EQUALITY-REPAIR.md"]
    paths += sorted(HERE.glob("*.py")) + sorted(HERE.glob("*.cg"))
    failed_attempts = []
    for path in sorted(evidence.glob("*.json")):
        record = json.loads(path.read_text())
        if record.get("passed") is False:
            failed_attempts.append({"record": str(path.relative_to(HERE)),
                                    "exit": record["exit"], "reason": record["reason"],
                                    "signal": record["signal"]})
    report = {"schema_version": 1, "base": BASE, "status": "bounded repair delivered for review",
              "gates": gates, "manifests": manifests,
              "sources_sha256": {str(p.relative_to(ROOT)): digest(p.read_bytes()) for p in paths},
              "retained_failed_attempts": failed_attempts,
              "attempt_notes": [
                  "Early test attempts failed to compile or used invalid fixture syntax; retained raw logs are not product defect evidence.",
                  "baseline-valid-cgx2-{runtime,sema} run valid regressions on unchanged graph.rs: runtime 3 failures and sema 1 failure reproduce the defect.",
                  "runtime-new-first has a Rust test child SIGABRT (6) from evaluator stack exhaustion in the initial deeply nested test setup. It is a failure, not expected success. Local bindings replaced the recursive setup; runtime-new-bounded and affected-tests pass the equality depth bound.",
                  "The first graph99/guards518/reports265 wrapper attempts used the same path for child product JSON and recorder JSON. The recorder raised after child success; original product JSON remains intact. The outer orchestration exited 1. Recheck attempts use distinct paths and retain complete command/log evidence.",
                  "typed-negative-precise detected a parser rejection in a new negative fixture. Corrected record/variant indentation now requires CGT039; typed-negative-corrected passes."],
              "managed_delivery_accepted": False, "full_memory_complete": False, "release_complete": False,
              "external_leak_audit_performance": "unrun; existing host limitations remain open",
              "scope": "Only graph.rs equality and owned new tests/evidence; no interfaces or manifests changed."}
    (HERE / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print("Verified final gates and source/log hashes; historical failures retained.")


if __name__ == "__main__":
    main()
