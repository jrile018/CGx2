import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import task_evidence as evidence


class TaskEvidenceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.candidate_root = Path(__file__).resolve().parents[1] / "fixtures/task-candidates/v3"
        self.candidate = json.loads((self.candidate_root / "pack.json").read_text())
        self.root.joinpath("private").mkdir()
        for language in ("cgx2", "rust"):
            workspace = self.root / language
            workspace.mkdir()
            for index in range(3):
                (workspace / f"file{index}.txt").write_text(f"{language}-{index}\n")
        (self.root / "private/hidden.py").write_text("assert answer == 7\n")
        self.task = copy.deepcopy(self.candidate["tasks"][0])
        self.task.update(status="frozen", reviewer_ids=["reviewer-a", "reviewer-b"],
                         reviewed_at_utc="2026-10-22T00:00:00Z",
                         hidden_test_path="private/hidden.py",
                         hidden_test_sha256=evidence.digest((self.root / "private/hidden.py").read_bytes()),
                         workspace_paths={"cgx2": "cgx2", "rust": "rust"},
                         repository_snapshots={language: evidence.tree_digest(self.root / language)[1]
                                               for language in ("cgx2", "rust")})
        prompt = self.root / "private/prompt.md"
        prompt.write_text("Repair the same quote in both languages.\n")
        self.task["prompt_path"] = "private/prompt.md"
        self.task["prompt_sha256"] = evidence.digest(prompt.read_bytes())
        self.task["equivalence"]["shared_prompt"] = self.task["prompt_path"]
        self.task["reference_paths"] = {}
        self.task["reference_snapshots"] = {}
        for language in ("cgx2", "rust"):
            reference = self.root / f"private/reference-{language}"
            reference.mkdir()
            (reference / "fix.txt").write_text("reference repair\n")
            self.task["reference_paths"][language] = str(reference.relative_to(self.root))
            self.task["reference_snapshots"][language] = evidence.tree_digest(reference)[1]
        self.task["build_commands"] = {
            "cgx2": ["cgx2c", "build", "--package", "cgx2", "--out", "/tmp/test-cgx2"],
            "rust": ["rustc", "rust/main.rs", "-o", "/tmp/test-rust"]}
        self.task["run_commands"] = {"cgx2": ["/tmp/test-cgx2"], "rust": ["/tmp/test-rust"]}
        self.task["acceptance_command"] = ["python3", "private/hidden.py", "--task", self.task["task_id"],
                                           "--language", "{language}", "--candidate-workspace", "{workspace}"]
        records = []
        for reviewer in self.task["reviewer_ids"]:
            path = self.root / f"private/{reviewer}.json"
            path.write_text(json.dumps({"task_id": self.task["task_id"],
                "reviewer_id": reviewer, "reviewed_at_utc": "2026-10-22T00:00:00Z",
                "approvals": {key: True for key in ("task", "equivalence", "hidden_test", "baseline")}}))
            records.append({"path": str(path.relative_to(self.root)),
                            "sha256": evidence.digest(path.read_bytes())})
        self.task["review_records"] = records
        sha = "a" * 64
        self.protocol = {key: sha for key in evidence.SHA_FIELDS}
        self.protocol.update(tool_version="shell-v1", hardware="test-host",
                             time_budget_seconds=100, token_budget=1000, randomization_seed=7)
        self.pack = {"schema_version": 3, "kind": "holdout", "protocol": self.protocol,
                     "tasks": [self.task], "expected_runs": [
                         {"run_id": f"run-{language}", "task_id": self.task["task_id"],
                          "language": language, "model_family": "family-a", "model_version": "v1",
                          "seed": 1, "order": order}
                         for order, language in enumerate(("cgx2", "rust"))]}
        self.freeze()

    def freeze(self):
        self.pack["freeze"] = {"frozen_at_utc": "2026-10-22T01:00:00Z",
                               "task_set_sha256": evidence.digest(evidence.canonical(self.pack["tasks"])),
                               "run_grid_sha256": evidence.digest(evidence.canonical(self.pack["expected_runs"])),
                               "protocol_sha256": evidence.digest(evidence.canonical(self.pack["protocol"]))}

    def write_run(self, language, outcome="pass", *, token_override=None):
        order = 0 if language == "cgx2" else 1
        run_id = f"run-{language}"
        tokenizer = {"tokenizer_id": "actual-tokenizer", "tokenizer_version": "v1",
                     "input_token_ids": [1, 2], "output_token_ids": [3], "tool_token_ids": [4]}
        token_path = self.root / f"private/{language}-tokens.json"
        token_path.write_text(json.dumps(tokenizer))
        raw = {"run_id": run_id, "events": [{"kind": "model_turn", "index": 0}],
               "verification": {"verifier_id": "external-hidden-runner-v1",
                                "hidden_test_ref": self.task["hidden_test_ref"],
                                "hidden_test_sha256": self.task["hidden_test_sha256"],
                                "result": outcome, "final_patch_sha256": "b" * 64}}
        raw_path = self.root / f"private/{language}-raw.json"
        raw_path.write_text(json.dumps(raw))
        run = {"schema_version": 3, **self.pack["expected_runs"][order], **self.protocol,
               "started_at_utc": f"2026-10-22T02:00:0{order}Z",
               "finished_at_utc": f"2026-10-22T02:00:1{order}Z",
               "hidden_test_ref": self.task["hidden_test_ref"],
               "hidden_test_sha256": self.task["hidden_test_sha256"],
               "repository_snapshot_sha256": self.task["repository_snapshots"][language],
               "tokenizer_id": "actual-tokenizer", "tokenizer_version": "v1",
               "tokenizer_output_path": str(token_path.relative_to(self.root)),
               "tokenizer_output_sha256": evidence.digest(token_path.read_bytes()),
               "prompt_tokens": 2, "completion_tokens": 1, "tool_tokens": 1,
               "total_tokens": 4 if token_override is None else token_override,
               "wall_seconds": 10, "outcome": outcome,
               "verified_completion": outcome == "pass",
               "failure_cause": None if outcome == "pass" else "hidden test failed",
               "final_patch_sha256": "b" * 64,
               "raw_trace_path": str(raw_path.relative_to(self.root)),
               "raw_trace_sha256": evidence.digest(raw_path.read_bytes())}
        path = self.root / f"private/{language}-run.json"
        path.write_text(json.dumps(run))
        return path

    def codes(self, run_paths=()):
        return [item["code"] for item in evidence.audit(self.pack, self.root, run_paths)["findings"]]

    def test_candidate_pack_cannot_support_claim(self):
        result = evidence.report(self.candidate, self.candidate_root)
        self.assertEqual(result["release_verdict"], "insufficient_evidence")
        self.assertEqual(result["summary"]["candidate_count"], 2)
        self.assertEqual(result["summary"]["tier_status_counts"],
                         {"small/candidate": 1, "large/candidate": 1})
        self.assertIsNone(result["comparison"])

    def test_incomplete_and_duplicate_pair(self):
        self.assertIn("missing_run", self.codes([self.write_run("cgx2")]))
        duplicate = copy.deepcopy(self.pack["expected_runs"][1])
        duplicate["run_id"] = "another-rust"
        self.pack["expected_runs"].append(duplicate)
        self.freeze()
        self.assertIn("duplicate_condition", self.codes())
        self.assertIn("duplicate_order", self.codes())

    def test_cgx2_cannot_be_its_own_baseline(self):
        self.task["baseline_language"] = "cgx2"
        self.freeze()
        self.assertIn("task_schema", self.codes())

    def test_hidden_leak_and_changed_freeze_are_rejected(self):
        self.task["hidden_test_path"] = "cgx2/hidden.py"
        (self.root / "cgx2/hidden.py").write_text("hidden\n")
        self.task["hidden_test_sha256"] = evidence.digest((self.root / "cgx2/hidden.py").read_bytes())
        self.task["repository_snapshots"]["cgx2"] = evidence.tree_digest(self.root / "cgx2")[1]
        self.freeze()
        self.assertIn("task_freeze", self.codes())
        self.task["baseline_language"] = "python"
        self.assertIn("freeze", self.codes())

    def test_tokenizer_mismatch_and_chronology_rejected(self):
        first = self.write_run("cgx2", token_override=3)
        second = self.write_run("rust")
        self.assertIn("run_provenance", self.codes([first, second]))
        run = json.loads(first.read_text())
        run["total_tokens"] = 4
        run["started_at_utc"] = "2026-10-21T00:00:00Z"
        first.write_text(json.dumps(run))
        self.assertIn("run_provenance", self.codes([first, second]))

    def test_failed_run_keeps_raw_evidence_and_full_budget_charge(self):
        paths = [self.write_run("cgx2", outcome="fail"), self.write_run("rust")]
        audited = evidence.audit(self.pack, self.root, paths)
        self.assertEqual(audited["validated_run_count"], 2)
        from holdout_analysis import _condition_summary
        failed = audited["validated_runs"][0]
        charged = _condition_summary([{**failed, "hidden_test_result": failed["outcome"]}])
        self.assertEqual(charged["charged_seconds"], 100)
        self.assertEqual(charged["charged_tokens"], 1000)
        self.assertEqual(evidence.report(self.pack, self.root, paths)["release_verdict"],
                         "insufficient_evidence")

    def test_ready_import_uses_existing_task_clustered_analysis(self):
        paths = [self.write_run("cgx2"), self.write_run("rust")]
        audited = evidence.audit(self.pack, self.root, paths)
        self.assertEqual(audited["validated_run_count"], 2)
        with patch.object(evidence, "audit", return_value={**audited, "status": "ready"}):
            result = evidence.report(self.pack, self.root, paths)
        self.assertEqual(result["comparison"]["comparison"]["matched_runs"], 1)
        self.assertEqual(result["release_verdict"], "uncertain")


if __name__ == "__main__":
    unittest.main()
