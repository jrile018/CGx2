import copy
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import matrix_audit


class MatrixAuditTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.workspace = self.root / "workspace"
        self.workspace.mkdir()
        for n in range(3):
            (self.workspace / f"file{n}.txt").write_text(f"starter {n}\n")
        self.hidden = self.root / "hidden.py"
        self.hidden.write_text("print('pass')\n")
        self.gold = self.root / "gold.patch"
        self.gold.write_text("public calibration gold marker\n")
        self.entries = []
        for language in ("cgx2", "python"):
            manifest = {"task_id": "task_a", "tier": "small", "language": language,
                        "variant": "standard", "model_id": "family-a/version-1", "seed": 1,
                        "cwd": "workspace", "argv": ["python3", "--version"],
                        "hidden_test_argv": ["python3", str(self.hidden)], "timeout_seconds": 10,
                        "budget": {"total_tokens": 1000}, "expected_exit": 0,
                        "versions": {"compiler": "pinned-1", "dependencies": "sha256:abc", "tools": "v1",
                                     "prompt": "v1", "hardware": "host-a", "threads": "1"},
                        "environment": {}, "trace_path": f"{language}-trace.json"}
            (self.root / manifest["trace_path"]).write_text(json.dumps({
                "output_tokens": 100, "total_tokens": 200, "iterations": 1, "human_interventions": 0}))
            path = self.root / f"{language}.json"
            raw = json.dumps(manifest).encode()
            path.write_bytes(raw)
            result = {"schema_version": 1, "task_id": "task_a", "tier": "small", "language": language,
                      "variant": "standard", "model_id": "family-a/version-1", "seed": 1,
                      "timeout_seconds": 10, "budget_total_tokens": 1000,
                      "verified_completion": True, "failure_reason": None, "wall_seconds": 2,
                      "charged_wall_seconds": 2, "output_tokens": 100, "total_tokens": 200,
                      "iterations": 1, "human_interventions": 0,
                      "exit_status": 0, "stdout_sha256": "0" * 64, "stderr_sha256": "0" * 64,
                      "hidden_test_exit_status": 0, "hidden_test_stdout_sha256": "0" * 64,
                      "hidden_test_stderr_sha256": "0" * 64,
                      "provenance": {"manifest_sha256": hashlib.sha256(raw).hexdigest(),
                                     "versions": manifest["versions"], "started_at_utc": "2026-10-22T00:00:00+00:00"}}
            result_path = self.root / f"{language}-result.json"
            result_path.write_text(json.dumps(result))
            self.entries.append({"manifest": path.name, "result": result_path.name,
                                 "workspace": "workspace", "hidden_tests": "hidden.py",
                                 "gold_changes": "gold.patch", "forbidden_markers": ["SECRET_GOLD"]})
        self.matrix = {"schema_version": 1, "kind": "public_calibration", "conditions":
                       [{"language": "cgx2", "variant": "standard"},
                        {"language": "python", "variant": "standard"}], "entries": self.entries}

    def audit(self):
        return matrix_audit.audit(self.matrix, self.root)

    def test_ready_shape_and_incomplete_pair(self):
        self.assertEqual(self.audit()["status"], "ready")
        self.matrix["entries"] = self.entries[:1]
        result = self.audit()
        self.assertEqual(result["status"], "incomplete")
        self.assertIn("unpaired_condition", [x["code"] for x in result["findings"]])

    def test_duplicate_condition_key_and_wrong_size(self):
        self.matrix["entries"].append(copy.deepcopy(self.entries[0]))
        self.assertIn("duplicate_key", [x["code"] for x in self.audit()["findings"]])
        self.matrix["entries"].pop()
        (self.workspace / "file1.txt").unlink()
        self.assertIn("starter_file_count", [x["code"] for x in self.audit()["findings"]])

    def test_hidden_leak_and_missing_trace(self):
        (self.workspace / "leak.txt").write_text("SECRET_GOLD")
        self.assertIn("source_leak_marker", [x["code"] for x in self.audit()["findings"]])
        (self.workspace / "leak.txt").unlink()
        path = self.root / "cgx2-result.json"
        data = json.loads(path.read_text())
        data["total_tokens"] = None
        path.write_text(json.dumps(data))
        result = self.audit()
        self.assertEqual(result["status"], "incomplete")
        self.assertIn("missing_trace", [x["code"] for x in result["findings"]])

    def test_hidden_path_and_history_leaks_are_invalid(self):
        self.entries[0]["hidden_tests"] = "workspace/file0.txt"
        self.assertIn("hidden_test_leak", [x["code"] for x in self.audit()["findings"]])
        self.entries[0]["hidden_tests"] = "hidden.py"
        (self.workspace / ".git").mkdir()
        self.assertIn("history_leak", [x["code"] for x in self.audit()["findings"]])

    def test_invalid_provenance_and_failed_run_charge(self):
        path = self.root / "cgx2-result.json"
        data = json.loads(path.read_text())
        data["provenance"]["manifest_sha256"] = "0" * 64
        path.write_text(json.dumps(data))
        self.assertIn("manifest_hash_mismatch", [x["code"] for x in self.audit()["findings"]])
        data["provenance"]["manifest_sha256"] = hashlib.sha256((self.root / "cgx2.json").read_bytes()).hexdigest()
        data["verified_completion"] = False
        data["failure_reason"] = "hidden_test_failed"
        path.write_text(json.dumps(data))
        self.assertIn("failed_budget_charge", [x["code"] for x in self.audit()["findings"]])


if __name__ == "__main__":
    unittest.main()
