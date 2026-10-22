import copy
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import holdout
import matrix_audit


SHA = "a" * 64


class HoldoutTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        starter = self.root / "starter"
        starter.mkdir()
        for number in range(3):
            (starter / f"file{number}.txt").write_text(f"starter {number}\n")
        (self.root / "statement.md").write_text("Repair the example.\n")
        (self.root / "hidden.py").write_text("PRIVATE_ORACLE = 42\n")
        (self.root / "gold.patch").write_text("PRIVATE_GOLD\n")
        self.task = {"task_id": "case-1", "tier": "small", "category": "repair",
                     "starter": "starter", "statement": "statement.md", "hidden_tests": "hidden.py",
                     "hidden_test_version": "v1",
                     "hidden_test_sha256": hashlib.sha256((self.root / "hidden.py").read_bytes()).hexdigest(),
                     "gold_changes": "gold.patch",
                     "baseline_language": "python", "applicable_languages": ["cgx2", "python"],
                     "exclusions": {"rust": "calibration only", "typescript": "calibration only",
                                    "ocaml": "calibration only"},
                     "forbidden_markers": ["PRIVATE_ORACLE", "PRIVATE_GOLD"]}
        self.matrix = {"schema_version": 2, "kind": "public_calibration",
                       "conditions": ["cgx2", "python", "rust", "typescript", "ocaml"],
                       "protocol": {"allowed_tools": "shell-v1", "documentation_budget": "docs-v1",
                                    "token_budget": 1000, "time_budget_seconds": 100,
                                    "hardware": "test-host", "toolchain": "test-toolchain"},
                       "tasks": [self.task], "runs": []}
        for language in ("cgx2", "python"):
            trace = self.trace(language)
            name = f"{language}.json"
            (self.root / name).write_text(json.dumps(trace))
            self.matrix["runs"].append({"task_id": "case-1", "language": language,
                                         "model_family": "family", "model_version": "v1",
                                         "run_id": f"run-{language}", "seed": 1,
                                         "order": 0 if language == "cgx2" else 1, "trace": name})

    def trace(self, language):
        return {"schema_version": 2, "task_id": "case-1", "hidden_test_version": "v1",
                "language": language, "model_family": "family", "model_version": "v1",
                "run_id": f"run-{language}", "seed": 1,
                "order": 0 if language == "cgx2" else 1,
                "allowed_tools": "shell-v1", "documentation_budget": "docs-v1",
                "token_budget": 1000, "time_budget_seconds": 100,
                "prompt_tokens": 100, "completion_tokens": 50, "wall_seconds": 20,
                "final_patch_sha256": SHA, "hidden_test_result": "pass",
                "failure_cause": None, "hardware": "test-host", "toolchain": "test-toolchain"}

    def audit(self):
        return holdout.audit(self.matrix, self.root)

    def test_complete_calibration_is_descriptive_only(self):
        self.assertEqual(self.audit()["status"], "ready")
        self.assertEqual(matrix_audit.audit(self.matrix, self.root)["status"], "ready")
        report = holdout.analyze(self.matrix, self.root)
        self.assertEqual(report["scope"], "public_calibration")
        self.assertIsNone(report["release_verdict"])
        self.assertEqual(report["comparison"]["completion_difference"]["estimate"], 0)

    def test_missing_trace_has_no_numeric_completion(self):
        (self.root / "cgx2.json").unlink()
        report = holdout.analyze(self.matrix, self.root)
        self.assertEqual(report["status"], "incomplete")
        self.assertIsNone(report["comparison"])
        self.assertIsNone(report["conditions"])

    def test_duplicate_ids_and_mixed_budgets_are_invalid(self):
        self.matrix["tasks"].append(copy.deepcopy(self.task))
        self.assertIn("duplicate_task_id", [x["code"] for x in self.audit()["findings"]])
        self.matrix["tasks"].pop()
        self.matrix["runs"][1]["run_id"] = "run-cgx2"
        self.assertIn("duplicate_run_id", [x["code"] for x in self.audit()["findings"]])
        self.matrix["runs"][1]["run_id"] = "run-python"
        data = self.trace("python")
        data["token_budget"] = 999
        (self.root / "python.json").write_text(json.dumps(data))
        self.assertIn("mixed_budget", [x["code"] for x in self.audit()["findings"]])

    def test_exclusions_and_hidden_leak(self):
        self.task["exclusions"].pop("rust")
        self.assertIn("applicability", [x["code"] for x in self.audit()["findings"]])
        self.task["exclusions"]["rust"] = "calibration only"
        (self.root / "starter" / "file0.txt").write_text("PRIVATE_ORACLE")
        self.assertIn("hidden_leak", [x["code"] for x in self.audit()["findings"]])

    def test_failure_cost_and_paired_success_filter(self):
        data = self.trace("python")
        data.update(hidden_test_result="fail", failure_cause="assertion", wall_seconds=1)
        (self.root / "python.json").write_text(json.dumps(data))
        report = holdout.analyze(self.matrix, self.root)
        comparison = report["comparison"]
        self.assertEqual(comparison["paired_successes"], 0)
        self.assertIsNone(comparison["time_ratio"]["estimate"])
        self.assertEqual(comparison["completion_difference"]["estimate"], 1)
        self.assertEqual(report["conditions"]["python"]["charged_seconds"], 100)
        self.assertEqual(report["conditions"]["python"]["charged_tokens"], 1000)

    def test_timeout_is_censored_and_ingest_checks_patch_and_version(self):
        data = self.trace("python")
        data.update(hidden_test_result="timeout", failure_cause="timeout", wall_seconds=100)
        (self.root / "python.json").write_text(json.dumps(data))
        self.assertEqual(self.audit()["status"], "ready")
        patch = self.root / "final.patch"
        patch.write_text("edit\n")
        data["final_patch_sha256"] = hashlib.sha256(patch.read_bytes()).hexdigest()
        data["hidden_test_version"] = "wrong"
        with self.assertRaises(ValueError):
            holdout.ingest(self.matrix, self.root, data, patch)
        data["hidden_test_version"] = "v1"
        self.assertEqual(holdout.ingest(self.matrix, self.root, data, patch)["run_id"], "run-python")
        data["final_patch_sha256"] = SHA
        with self.assertRaises(ValueError):
            holdout.ingest(self.matrix, self.root, data, patch)

    def test_hidden_test_digest_change_is_invalid(self):
        (self.root / "hidden.py").write_text("changed oracle\n")
        self.assertIn("fixture_path", [x["code"] for x in self.audit()["findings"]])

    def test_committed_public_template_refuses_claim(self):
        matrix_path = Path(__file__).resolve().parents[1] / "fixtures/holdout/v1/matrix.json"
        matrix = json.loads(matrix_path.read_text())
        report = holdout.analyze(matrix, matrix_path.parent)
        self.assertEqual(report["status"], "incomplete")
        self.assertIsNone(report["comparison"])
        self.assertEqual(report["audit"]["validated_run_count"], 0)

    def test_intervals_and_gates_from_synthetic_30_tasks(self):
        template = copy.deepcopy(self.task)
        self.matrix["tasks"] = []
        self.matrix["runs"] = []
        for index in range(30):
            task = copy.deepcopy(template)
            task["task_id"] = f"case-{index}"
            self.matrix["tasks"].append(task)
            for language in ("cgx2", "python"):
                trace = self.trace(language)
                trace.update(task_id=task["task_id"], run_id=f"{task['task_id']}-{language}",
                             prompt_tokens=40 if language == "cgx2" else 100,
                             completion_tokens=20 if language == "cgx2" else 50,
                             wall_seconds=10 if language == "cgx2" else 20)
                name = f"{task['task_id']}-{language}.json"
                (self.root / name).write_text(json.dumps(trace))
                self.matrix["runs"].append({k: trace[k] for k in
                                            ("task_id", "language", "model_family", "model_version",
                                             "run_id", "seed", "order")} | {"trace": name})
        report = holdout.analyze(self.matrix, self.root)
        self.assertEqual(report["comparison"]["time_ratio"]["confidence_interval_95"], [0.5, 0.5])
        self.assertEqual(report["comparison"]["completion_difference"]["interval_status"], "computed")
        self.assertIsNone(report["release_verdict"])

    def test_synthetic_holdout_gate_requires_full_review_shape(self):
        for tier, size in (("small", 3), ("large", 20)):
            for language in ("cgx2", "python"):
                starter = self.root / f"{tier}-{language}"
                starter.mkdir()
                for number in range(size):
                    (starter / f"module{number}.txt").write_text(f"module {number}\n")
        template = copy.deepcopy(self.task)
        self.matrix["kind"] = "holdout"
        self.matrix["tasks"] = []
        self.matrix["runs"] = []
        categories = ("memory", "modularity", "foreign_library", "comprehension_repair")
        for number in range(60):
            task = copy.deepcopy(template)
            task["task_id"] = f"holdout-{number}"
            task["tier"] = "small" if number % 2 == 0 else "large"
            task["category"] = categories[number % 4]
            task.pop("starter")
            task["starters"] = {language: f"{task['tier']}-{language}" for language in ("cgx2", "python")}
            self.matrix["tasks"].append(task)
            for family in ("family-a", "family-b"):
                for seed in (1, 2):
                    for order, language in enumerate(("cgx2", "python")):
                        trace = self.trace(language)
                        trace.update(task_id=task["task_id"], model_family=family, seed=seed,
                                     run_id=f"{number}-{family}-{seed}-{language}", order=order,
                                     prompt_tokens=40 if language == "cgx2" else 100,
                                     completion_tokens=20 if language == "cgx2" else 50,
                                     wall_seconds=10 if language == "cgx2" else 20)
                        name = f"{trace['run_id']}.json"
                        (self.root / name).write_text(json.dumps(trace))
                        self.matrix["runs"].append({k: trace[k] for k in
                                                    ("task_id", "language", "model_family", "model_version",
                                                     "run_id", "seed", "order")} | {"trace": name})
        task_bytes = json.dumps(self.matrix["tasks"], sort_keys=True, separators=(",", ":"),
                                ensure_ascii=False).encode()
        acceptance_bytes = json.dumps(sorted((task["task_id"], task["hidden_test_sha256"])
                                              for task in self.matrix["tasks"]), separators=(",", ":")).encode()
        self.matrix["freeze"] = {"task_set_sha256": hashlib.sha256(task_bytes).hexdigest(),
                                 "acceptance_sha256": hashlib.sha256(acceptance_bytes).hexdigest(),
                                 "prompts_sha256": SHA, "tools_sha256": SHA, "locks_sha256": SHA,
                                 "order_seed": 1, "frozen_at_utc": "2026-10-22T00:00:00Z",
                                 "reviewer_ids": ["synthetic-test-only"]}
        report = holdout.analyze(self.matrix, self.root)
        self.assertEqual(report["status"], "ready")
        self.assertEqual(report["release_verdict"], "gates_met")
        self.assertEqual(report["selected_baseline"]["runs"], 240)
        self.assertEqual(report["comparison"]["total_token_ratio"]["confidence_interval_95"], [0.4, 0.4])
        self.matrix["tasks"][0]["baseline_language"] = "rust"
        self.assertIn("task_freeze_mismatch", [x["code"] for x in self.audit()["findings"]])


if __name__ == "__main__":
    unittest.main()
