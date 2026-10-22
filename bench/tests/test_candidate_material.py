import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import task_evidence as evidence


class CandidateMaterialTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = Path(__file__).resolve().parents[1] / "fixtures/task-candidates/v3"
        cls.original = json.loads((cls.root / "pack.json").read_text())

    def pack(self):
        return copy.deepcopy(self.original)

    def findings(self, pack):
        return evidence.audit(pack, self.root)["findings"]

    def test_real_candidate_material_is_hash_pinned_and_unscored(self):
        result = evidence.audit(self.pack(), self.root)
        self.assertEqual(result["status"], "insufficient_evidence")
        self.assertEqual(result["task_count"], 2)
        self.assertEqual(result["candidate_count"], 2)
        self.assertNotIn("candidate_material", [item["code"] for item in result["findings"]])

    def test_stale_starter_and_hidden_hashes_are_rejected(self):
        pack = self.pack()
        pack["tasks"][0]["repository_snapshots"]["cgx2"] = "0" * 64
        self.assertIn("candidate_material", [item["code"] for item in self.findings(pack)])
        pack = self.pack()
        pack["tasks"][0]["hidden_test_sha256"] = "0" * 64
        self.assertIn("candidate_material", [item["code"] for item in self.findings(pack)])

    def test_hidden_leak_and_equivalence_drift_are_rejected(self):
        pack = self.pack()
        pack["tasks"][0]["hidden_test_path"] = "tasks/quote-credit-v3/cgx2/src/main.cg"
        pack["tasks"][0]["hidden_test_sha256"] = evidence.digest(
            (self.root / pack["tasks"][0]["hidden_test_path"]).read_bytes())
        self.assertIn("candidate_material", [item["code"] for item in self.findings(pack)])
        pack = self.pack()
        pack["tasks"][0]["equivalence"]["same_cases_ref"] = "different"
        self.assertIn("candidate_material", [item["code"] for item in self.findings(pack)])

    def test_commands_and_file_counts_are_required(self):
        pack = self.pack()
        pack["tasks"][0]["build_commands"]["rust"] = []
        self.assertIn("candidate_material", [item["code"] for item in self.findings(pack)])
        pack = self.pack()
        pack["tasks"][0]["acceptance_command"] = pack["tasks"][0]["acceptance_command"][:4]
        self.assertIn("candidate_material", [item["code"] for item in self.findings(pack)])
        pack = self.pack()
        pack["tasks"][0]["tier"] = "large"
        pack["tasks"][0]["planned_repository_files"] = 20
        self.assertIn("candidate_material", [item["code"] for item in self.findings(pack)])

    def test_frozen_status_needs_independent_review_artifacts(self):
        task = self.pack()["tasks"][0]
        task["reviewer_ids"] = ["reviewer-a", "reviewer-b"]
        with self.assertRaisesRegex(ValueError, "review records"):
            evidence.review_records(task, self.root, evidence.utc("2026-10-22T00:00:00Z"))


if __name__ == "__main__":
    unittest.main()
